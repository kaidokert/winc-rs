// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::{StackError, WincClient, Xfer};
use embedded_nal::nb;

/// 1 second timeout to read an ethernet packet.
const ETHERNET_RX_TIMEOUT_MSEC: u32 = 1000;

impl<X: Xfer> WincClient<'_, X> {
    /// Tries to read an Ethernet packet from the module within a specified timeout.
    ///
    /// # Note
    ///
    /// The user application is responsible for parsing the Ethernet packet.
    ///
    /// # Arguments
    ///
    /// * `buffer` - An optional mutable slice used to store the received Ethernet packet.
    ///   If `None`, the internal receive buffer with a capacity of `SOCKET_BUFFER_MAX_LENGTH`
    ///   bytes is used.
    /// * `timeout` - An optional timeout in milliseconds, to wait for an Ethernet packet.
    ///   If `None`, the default timeout value `ETHERNET_RX_TIMEOUT_MSEC` is used.
    ///
    /// # Returns
    ///
    /// * `Ok(usize)` - The number of bytes read from the module.
    /// * `Err(StackError)` - If an error occurs while reading the ethernet packet.
    pub fn read_ethernet_packet(
        &mut self,
        buffer: Option<&mut [u8]>,
        timeout: Option<u32>,
    ) -> nb::Result<usize, StackError> {
        match &mut self.callbacks.eth_rx_info {
            None => {
                self.callbacks.eth_rx_info = Some(None);
                let timeout_ms = timeout.unwrap_or(ETHERNET_RX_TIMEOUT_MSEC);
                // todo clean-up
                self.operation_countdown = (timeout_ms * 1000) / self.poll_loop_delay_us;
            }
            Some(info) => {
                if let Some(info) = info {
                    let recv_buffer = match buffer {
                        None => {
                            self.callbacks.recv_buffer.fill(0);
                            self.callbacks.recv_buffer.as_mut_slice()
                        }
                        Some(buffer) => buffer,
                    };
                    let len_to_read = recv_buffer.len().min(info.packet_size as usize);
                    let rx_done = len_to_read >= info.packet_size as usize;
                    self.manager.recv_ethernet_packet(
                        info.hif_address + info.data_offset as u32,
                        &mut recv_buffer[..len_to_read],
                        rx_done,
                    )?;
                    // check if all data is read from the module.
                    if rx_done {
                        // no bytes left to read
                        self.callbacks.eth_rx_info = None;
                    } else {
                        info.data_offset += len_to_read as u16;
                        info.packet_size -= len_to_read as u16;
                    }

                    return Ok(len_to_read);
                } else {
                    self.delay_us(self.poll_loop_delay_us);
                    if self.operation_countdown == 0 {
                        self.callbacks.eth_rx_info = None;
                        return Err(nb::Error::Other(StackError::GeneralTimeout));
                    }
                    self.operation_countdown -= 1;
                }
            }
        }
        self.dispatch_events()?;
        Err(nb::Error::WouldBlock)
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
        Ok(self.manager.send_ethernet_packet(net_pkt)?)
    }
}

#[cfg(feature = "smoltcp")]
mod smoltcp_impl {
    use super::{WincClient, Xfer};
    use crate::error;
    use crate::manager::{Manager, SOCKET_BUFFER_MAX_LENGTH};
    use embedded_nal::nb;
    use smoltcp::{
        phy::{self, DeviceCapabilities, Medium},
        time::Instant,
    };
    // 100 milliseconds timeout to wait for ethernet packet to arrive.
    const ETH_RECV_TIMEOUT_MSEC: u32 = 100;

    /// Container for sending a single network packet.
    pub struct WincTxToken<'a, X: Xfer> {
        client: Option<&'a mut Manager<X>>,
    }

    /// Container for receiving a single network packet.
    pub struct WincRxToken<'a> {
        buffer: &'a mut [u8],
        read_length: usize,
    }

    /// Implementation of an interface for sending and receiving raw network frames.
    impl<X: Xfer> phy::Device for WincClient<'_, X> {
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
            _timestamp: Instant,
        ) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
            let result = nb::block!(self.read_ethernet_packet(None, Some(ETH_RECV_TIMEOUT_MSEC)));

            let Ok(read_length) = result else {
                return None;
            };

            let rx_token = WincRxToken {
                buffer: &mut self.callbacks.recv_buffer,
                read_length,
            };

            let tx_token = WincTxToken {
                client: Some(&mut self.manager),
            };
            Some((rx_token, tx_token))
        }

        /// Construct a transmit token.
        fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
            let tx_token = WincTxToken {
                client: Some(&mut self.manager),
            };
            Some(tx_token)
        }

        /// Get a description of device capabilities.
        fn capabilities(&self) -> DeviceCapabilities {
            let mut caps = DeviceCapabilities::default();
            caps.max_transmission_unit = SOCKET_BUFFER_MAX_LENGTH;
            caps.max_burst_size = Some(1);
            caps.medium = Medium::Ethernet;
            caps
        }
    }

    impl<'a> phy::RxToken for WincRxToken<'a> {
        /// Consumes the token to receive a single network packet.
        fn consume<R, F>(self, f: F) -> R
        where
            F: FnOnce(&[u8]) -> R,
        {
            let length = self.read_length;
            f(&self.buffer[..length])
        }
    }

    impl<'a, X: Xfer> phy::TxToken for WincTxToken<'a, X> {
        /// Consumes the token to send a single network packet.
        fn consume<R, F>(self, len: usize, f: F) -> R
        where
            F: FnOnce(&mut [u8]) -> R,
        {
            let mut tx_buffer = [0u8; SOCKET_BUFFER_MAX_LENGTH];
            let result = f(&mut tx_buffer[..len]);

            if let Some(manager) = self.client {
                let _ = manager.send_ethernet_packet(&tx_buffer[..len]);
            } else {
                error!("No client available to send the packet.");
            }

            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{test_shared::*, SocketCallbacks};
    use crate::manager::{EventListener, MAX_TX_ETHERNET_PACKET_SIZE};
    use crate::CommError;

    #[test]
    fn test_send_ethernet_packet_success() {
        let mut client = make_test_client();
        let packet = [0xffu8; 10];

        let result = client.send_ethernet_packet(&packet);

        assert!(result.is_ok());
    }

    #[test]
    fn test_send_ethernet_packet_failed() {
        let mut client = make_test_client();
        let result = client.send_ethernet_packet(&[]);

        assert_eq!(
            result,
            Err(StackError::WincWifiFail(CommError::BufferError))
        );
    }

    #[test]
    fn test_read_ethernet_packet_success() {
        let mut client = make_test_client();
        let rx_info = (100 as u16, 111 as u16, 0xAABBCCDD as u32);
        let mut rx_buffer = [0u8; 200];

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_eth(rx_info.0, rx_info.1, rx_info.2);
        };
        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.read_ethernet_packet(Some(&mut rx_buffer), None));

        assert!(result.is_ok());
    }

    #[test]
    fn test_read_ethernet_packet_timeout() {
        let mut client = make_test_client();
        let mut rx_buffer = [0u8; 200];
        let timeout = 1000 as u32;

        let result = nb::block!(client.read_ethernet_packet(Some(&mut rx_buffer), Some(timeout)));

        assert_eq!(result, Err(StackError::GeneralTimeout));
    }

    #[test]
    fn test_read_ethernet_packet_internal_buffer() {
        let mut client = make_test_client();
        client.callbacks.recv_buffer.fill(0xff);
        let rx_info = (1600 as u16, 111 as u16, 0xAABBCCDD as u32);
        let timeout = 1000 as u32;

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_eth(rx_info.0, rx_info.1, rx_info.2);
        };
        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.read_ethernet_packet(None, Some(timeout)));

        assert!(result.is_ok());
        assert!(client.callbacks.recv_buffer.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_read_ethernet_over_range() {
        let mut client = make_test_client();
        let packet = [0u8; MAX_TX_ETHERNET_PACKET_SIZE + 1];
        let result = client.send_ethernet_packet(&packet);

        assert_eq!(
            result,
            Err(StackError::WincWifiFail(CommError::BufferError))
        );
    }
}
