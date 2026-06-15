use super::AsyncClient;
use crate::ops::{module::SyncOp, net_ops::ethernet_receive::RxEthernetPacketInfo};
use crate::stack::StackError;
use crate::transfer::Xfer;

impl<X: Xfer> AsyncClient<'_, X> {
    /// Tries to read an Ethernet packet from the module within a specified timeout.
    ///
    /// # Note
    ///
    /// The user application is responsible for parsing the Ethernet packet.
    ///
    /// # Arguments
    ///
    /// * `buffer` - A mutable slice used to store the received Ethernet packet.
    /// * `timeout` - An optional duration to wait for a packet.
    ///   If `None`, the default timeout value `ETHERNET_RX_TIMEOUT_MSEC` is used.
    ///
    /// # Returns
    ///
    /// * `Ok(usize)` - The number of bytes read from the module.
    /// * `Err(StackError)` - If an error occurs while reading the ethernet packet.
    pub async fn read_ethernet_packet(
        &mut self,
        buffer: Option<&mut [u8]>,
        timeout: Option<u32>,
    ) -> Result<usize, StackError> {
        let mut op = RxEthernetPacketInfo::new(buffer, timeout);
        self.poll_op(&mut op).await
    }

    /// Sends an Ethernet packet to the module.
    ///
    /// # Note
    ///
    /// The user application is responsible for constructing the Ethernet packet.
    ///
    /// # Arguments
    ///
    /// * `net_pkt` - The raw Ethernet packet data to be transmitted.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If packet is successfully sent to the module.
    /// * `Err(StackError)` - If an error occurred while sending the ethernet packet.
    pub fn send_ethernet_packet(&mut self, net_pkt: &[u8]) -> Result<(), StackError> {
        let mut op = SyncOp::send_ethernet_packet(net_pkt);
        self.poll_once(&mut op)
    }
}

#[cfg(feature = "embassy-net")]
mod embassy_net {
    use super::{AsyncClient, Xfer};
    use crate::error;
    use crate::manager::{Manager, MAX_OCTETS_IN_MAC_ADDRESS, SOCKET_BUFFER_MAX_LENGTH};
    use crate::ops::net_ops::ethernet_receive::RxEthernetPacketInfo;
    use crate::stack::socket_callbacks::SocketCallbacks;
    use core::cell::RefCell;
    use core::task::Context;
    use embassy_net_driver::{Capabilities, HardwareAddress, LinkState};

    // 100 milliseconds timeout to wait for ethernet packet to arrive.
    const ETH_RECV_TIMEOUT_MSEC: u32 = 100;
    /// Default Mac address
    const DEFAULT_MAC_ADDRESS: [u8; MAX_OCTETS_IN_MAC_ADDRESS] = [00, 0x1E, 0xC0, 00, 00, 00];

    /// Container for sending a single network packet.
    pub struct WincTxToken<'a, X: Xfer> {
        client: &'a RefCell<Manager<X>>,
    }

    /// Container for receiving a single network packet.
    pub struct WincRxToken<'a> {
        callback: &'a RefCell<SocketCallbacks>,
        read_length: usize,
    }

    /// Implementation of an interface for sending and receiving raw network frames.
    impl<X: Xfer> embassy_net_driver::Driver for AsyncClient<'_, X> {
        type RxToken<'a>
            = WincRxToken<'a>
        where
            Self: 'a;

        type TxToken<'a>
            = WincTxToken<'a, X>
        where
            Self: 'a;

        /// Construct a token pair consisting of one receive token and one transmit token.
        fn receive(
            &mut self,
            cx: &mut Context<'_>,
        ) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
            // poll for new events
            let _ = self.heartbeat();

            // poll for a packet
            let mut rx_op = RxEthernetPacketInfo::new(None, Some(ETH_RECV_TIMEOUT_MSEC));
            let result = self.poll_once(&mut rx_op);

            let Ok(read_length) = result else {
                // Waker is registered, poll the winc heatbeat before polling the embassy interface.TODO: make this clearer
                let _ = self.manager.borrow_mut().register_waker(cx.waker().clone());
                return None;
            };

            // unregister the waker
            self.manager.borrow_mut().unregister_waker(&cx.waker());

            let rx_token = WincRxToken {
                callback: &self.callbacks,
                read_length,
            };

            let tx_token = WincTxToken {
                client: &self.manager,
            };
            Some((rx_token, tx_token))
        }

        /// Construct a transmit token.
        fn transmit(&mut self, _cx: &mut Context<'_>) -> Option<Self::TxToken<'_>> {
            let tx_token = WincTxToken {
                client: &self.manager,
            };
            Some(tx_token)
        }

        /// Get a description of device capabilities.
        fn capabilities(&self) -> Capabilities {
            let mut cap = Capabilities::default();
            cap.max_transmission_unit = SOCKET_BUFFER_MAX_LENGTH;
            cap.max_burst_size = Some(1);

            cap
        }

        fn link_state(&mut self, _cx: &mut core::task::Context) -> LinkState {
            // TODO:
            LinkState::Up
        }

        fn hardware_address(&self) -> HardwareAddress {
            match self.get_winc_mac_address() {
                Ok(mac) => HardwareAddress::Ethernet(mac.octets()),
                Err(_) => HardwareAddress::Ethernet(DEFAULT_MAC_ADDRESS),
            }
        }
    }

    impl<'a> embassy_net_driver::RxToken for WincRxToken<'a> {
        /// Consumes the token to receive a single network packet.
        fn consume<R, F>(self, f: F) -> R
        where
            F: FnOnce(&mut [u8]) -> R,
        {
            let length = self.read_length;
            f(&mut self.callback.borrow_mut().recv_buffer[..length])
        }
    }

    impl<'a, X: Xfer> embassy_net_driver::TxToken for WincTxToken<'a, X> {
        /// Consumes the token to send a single network packet.
        fn consume<R, F>(self, len: usize, f: F) -> R
        where
            F: FnOnce(&mut [u8]) -> R,
        {
            let mut tx_buffer = [0u8; SOCKET_BUFFER_MAX_LENGTH];
            let result = f(&mut tx_buffer[..len]);

            if let Err(e) = self
                .client
                .borrow_mut()
                .send_ethernet_packet(&tx_buffer[..len])
            {
                error!("Failed to send ethernet packet: {:?}", e);
            }

            result
        }
    }
}
