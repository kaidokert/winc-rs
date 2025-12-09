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

use super::{EventListener, HifGroup, IpCode, Manager, WifiResponse};
use crate::errors::CommError as Error;
use crate::manager::constants::{
    PRNG_DATA_LENGTH, PRNG_PACKET_SIZE, PROVISIONING_INFO_PACKET_SIZE, SOCKET_BUFFER_MAX_LENGTH,
};

#[cfg(feature = "ssl")]
use super::constants::{SslResponse, SSL_CS_MAX_PACKET_SIZE};

#[cfg(feature = "experimental-ecc")]
use super::constants::SSL_ECC_REQ_PACKET_SIZE;

use crate::manager::responses::*;
use crate::{error, transfer::Xfer};

#[cfg(feature = "experimental-ota")]
use crate::manager::constants::OtaResponse;

#[cfg(feature = "ethernet")]
use crate::manager::{constants::NET_XFER_PACKET_SIZE, HIF_HEADER_OFFSET};

impl<X: Xfer> Manager<X> {
    /// Parses incoming WiFi events from the chip and dispatches them to the provided event listener.
    ///
    /// # Arguments
    ///
    /// * `listener` - The event callback handler that will be invoked based on the event type.
    /// * `address` - The register address of the module from which data can be read.
    /// * `wifi_res` - The WiFi response ID indicating the type of event.
    ///
    /// # Returns
    ///
    /// * `()` - If no error occurred while processing the events.
    /// * `Error` - If an error occurred while processing the events.
    fn wifi_events_listener<T: EventListener>(
        &mut self,
        listener: &mut T,
        address: u32,
        wifi_res: WifiResponse,
    ) -> Result<(), Error> {
        match wifi_res {
            WifiResponse::CurrentRssi => {
                let mut result = [0xff; 4];
                self.read_block(address, &mut result)?;
                listener.on_rssi(result[0] as i8)
            }
            WifiResponse::DefaultConnect => {
                let mut def_connect = [0xff; 4];
                self.read_block(address, &mut def_connect)?;
                listener.on_default_connect(def_connect[0].into())
            }
            WifiResponse::DhcpConf => {
                let mut result = [0xff; 20];
                self.read_block(address, &mut result)?;
                listener.on_dhcp(read_dhcp_conf(&result)?)
            }
            WifiResponse::ConStateChanged => {
                let mut connstate = [0xff; 4];
                self.read_block(address, &mut connstate)?;
                listener.on_connstate_changed(connstate[0].into(), connstate[1].into());
            }
            WifiResponse::ConnInfo => {
                let mut conninfo = [0xff; 48];
                self.read_block(address, &mut conninfo)?;
                listener.on_connection_info(conninfo.into())
            }
            WifiResponse::ScanResult => {
                let mut result = [0xff; 44];
                self.read_block(address, &mut result)?;
                listener.on_scan_result(result.into())
            }
            WifiResponse::ScanDone => {
                let mut result = [0xff; 0x4];
                self.read_block(address, &mut result)?;
                listener.on_scan_done(result[0], result[1].into())
            }
            WifiResponse::ClientInfo => {
                unimplemented!("PS mode not yet supported")
            }
            // could translate to embedded-time, or core::Duration. No core::Systemtime exists
            // or chrono::
            WifiResponse::GetSysTime => {
                let mut result = [0xff; 8];
                self.read_block(address, &mut result)?;
                listener.on_system_time(
                    (result[1] as u16 * 256u16) + result[0] as u16,
                    result[2],
                    result[3],
                    result[4],
                    result[5],
                    result[6],
                );
            }
            WifiResponse::IpConflict => {
                // replies with 4 bytes of conflicted IP
                let mut result = [0xff; 4];
                self.read_block(address, &mut result)?;
                listener.on_ip_conflict(u32::from_be_bytes(result).into());
            }
            WifiResponse::ProvisionInfo => {
                let mut response = [0u8; PROVISIONING_INFO_PACKET_SIZE];
                // read the provisioning info
                self.read_block(address, &mut response)?;
                let res = read_provisioning_reply(&response)?;
                listener.on_provisioning(res.0, res.1, (res.2).into(), res.3);
            }
            WifiResponse::GetPrng => {
                let mut response = [0; PRNG_DATA_LENGTH];
                // read the prng packet
                self.read_block(address, &mut response[0..PRNG_PACKET_SIZE])?;

                let (_, len) = read_prng_reply(&response)?;
                // read the random bytes
                self.read_block(
                    address + PRNG_PACKET_SIZE as u32,
                    &mut response[0..len as usize],
                )?;
                listener.on_prng(&response[0..len as usize]);
            }
            WifiResponse::EthernetRxPacket => {
                #[cfg(feature = "ethernet")]
                {
                    let mut response = [0u8; NET_XFER_PACKET_SIZE];

                    self.recv_ethernet_packet(
                        address + HIF_HEADER_OFFSET as u32,
                        &mut response,
                        false,
                    )?;
                    let reply = read_eth_rx_reply(&response)?;
                    listener.on_eth(reply.0, reply.1, address + HIF_HEADER_OFFSET as u32);
                }
                #[cfg(not(feature = "ethernet"))]
                {
                    error!("Ethernet feature not enabled: WiFi HIF: {:?}", wifi_res);
                    return Err(Error::InvalidHifResponse("WiFi"));
                }
            }
            WifiResponse::Unhandled | WifiResponse::Wps | WifiResponse::WifiRxPacket => {
                error!("Unhandled Wifi HIF: {:?}", wifi_res);
                return Err(Error::InvalidHifResponse("WiFi"));
            }
        }
        Ok(())
    }

    #[cfg(feature = "experimental-ota")]
    /// Parses incoming OTA events from the chip and dispatches them to the provided event listener.
    ///
    /// # Arguments
    ///
    /// * `listener` - The event callback handler that will be invoked based on the event type.
    /// * `address` - The register address of the module from which data can be read.
    /// * `ota_res` - The OTA response ID indicating the type of event.
    ///
    /// # Returns
    ///
    /// * `()` - If no error occurred while processing the events.
    /// * `Error` - If an error occurred while processing the events.
    fn ota_events_listener<T: EventListener>(
        &mut self,
        listener: &mut T,
        address: u32,
        ota_res: OtaResponse,
    ) -> Result<(), Error> {
        match ota_res {
            OtaResponse::OtaNotifyUpdateInfo => {
                todo!("OTA Notify is not supported")
            }
            OtaResponse::OtaUpdateStatus => {
                let mut response = [0u8; 4];
                self.read_block(address, &mut response)?;
                listener.on_ota(response[0].into(), response[1].into());
            }
            _ => {
                error!("Received invalid OTA response: {:?}", ota_res);
                return Err(Error::InvalidHifResponse("OTA"));
            }
        }

        Ok(())
    }

    /// Parses incoming IP events from the chip and dispatches them to the provided event listener.
    ///
    /// # Arguments
    ///
    /// * `listener` - The event callback handler that will be invoked based on the event type.
    /// * `address` - The register address of the module from which data can be read.
    /// * `ip_res` - The IP response ID indicating the type of event.
    ///
    /// # Returns
    ///
    /// * `()` - If no error occurred while processing the events.
    /// * `Error` - If an error occurred while processing the events.
    fn ip_events_listener<T: EventListener>(
        &mut self,
        listener: &mut T,
        address: u32,
        ip_res: IpCode,
    ) -> Result<(), Error> {
        match ip_res {
            IpCode::DnsResolve => {
                let mut result = [0; 68];
                self.read_block(address, &mut result)?;
                let rep = read_dns_reply(&result)?;
                listener.on_resolve(rep.0, &rep.1);
            }
            IpCode::Ping => {
                let mut result = [0; 20];
                self.read_block(address, &mut result)?;
                let rep = read_ping_reply(&result)?;
                listener.on_ping(rep.0, rep.1, rep.2, rep.3, rep.4, rep.5)
            }
            IpCode::Bind => {
                let mut result = [0; 4];
                self.read_block(address, &mut result)?;
                let rep = read_common_socket_reply(&result)?;
                listener.on_bind(rep.0, rep.1);
            }
            #[cfg(feature = "ssl")]
            IpCode::SslBind => {
                let mut result = [0; 4];
                self.read_block(address, &mut result)?;
                let rep = read_common_socket_reply(&result)?;
                listener.on_bind(rep.0, rep.1);
            }
            IpCode::Listen => {
                let mut result = [0; 4];
                self.read_block(address, &mut result)?;
                let rep = read_common_socket_reply(&result)?;
                listener.on_listen(rep.0, rep.1);
            }
            IpCode::Accept => {
                let mut result = [0; 12];
                self.read_block(address, &mut result)?;
                let rep = read_accept_reply(&result)?;
                listener.on_accept(rep.0, rep.1, rep.2, rep.3);
            }
            // The reply for `IpCode::SslConnect` is the same as for `IpCode::Connect`.
            IpCode::Connect => {
                let mut result = [0; 4];
                self.read_block(address, &mut result)?;
                let rep = read_connect_socket_reply(&result)?;
                listener.on_connect(rep.0, rep.1)
            }
            IpCode::SendTo => {
                let mut result = [0; 8];
                self.read_block(address, &mut result)?;
                let rep = read_send_reply(&result)?;
                listener.on_send_to(rep.0, rep.1)
            }
            // The reply for `IpCode::SslSend` is the same as for `IpCode::Send`.
            IpCode::Send => {
                let mut result = [0; 8];
                self.read_block(address, &mut result)?;
                let rep = read_send_reply(&result)?;
                listener.on_send(rep.0, rep.1)
            }
            IpCode::Recv => {
                let mut buffer = [0; SOCKET_BUFFER_MAX_LENGTH];
                let rep = self.get_recv_reply(address, &mut buffer)?;
                listener.on_recv(rep.0, rep.1, rep.2, rep.3)
            }
            IpCode::RecvFrom => {
                let mut buffer = [0; SOCKET_BUFFER_MAX_LENGTH];
                let rep = self.get_recv_reply(address, &mut buffer)?;
                listener.on_recvfrom(rep.0, rep.1, rep.2, rep.3)
            }
            #[cfg(feature = "ssl")]
            IpCode::SslRecv => {
                let mut buffer = [0; SOCKET_BUFFER_MAX_LENGTH];
                let rep = self.get_recv_reply(address, &mut buffer)?;
                listener.on_recv(rep.0, rep.1, rep.2, rep.3)
            }
            IpCode::Close => {
                unimplemented!("There is no response for close")
            }
            IpCode::SetSocketOption => {
                unimplemented!("There is no response for setsockoption")
            }
            _ => {
                error!("Received unhandled IP HIF code: {:?}", ip_res);
                return Err(Error::InvalidHifResponse("IP"));
            }
        }
        Ok(())
    }

    /// Parses incoming SSL events from the chip and dispatches them to the provided event listener.
    ///
    /// # Arguments
    ///
    /// * `listener` - The event callback handler that will be invoked based on the SSL event type.
    /// * `address` - The register address of the module from which data can be read.
    /// * `ssl_res` - The SSL response ID indicating the type of event.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If no error occurred while processing the events.
    /// * `Err(Error)` - If an error occurred while processing the events.
    #[cfg(feature = "ssl")]
    fn ssl_events_listener<T: EventListener>(
        &mut self,
        listener: &mut T,
        address: u32,
        ssl_res: SslResponse,
    ) -> Result<(), Error> {
        match ssl_res {
            #[cfg(feature = "experimental-ecc")]
            SslResponse::EccReqUpdate => {
                let mut response = [0u8; SSL_ECC_REQ_PACKET_SIZE];
                // read the ecc request
                self.read_block(address, &mut response)?;
                // parse the ecc request
                let mut ecc_req = read_ssl_ecc_response(&response)?;
                // set the hif register for the ecc.
                ecc_req.hif_reg = address + SSL_ECC_REQ_PACKET_SIZE as u32;
                listener.on_ssl(ssl_res, None, Some(ecc_req));
            }
            SslResponse::CipherSuiteUpdate => {
                let mut response = [0u8; SSL_CS_MAX_PACKET_SIZE];
                self.read_block(address, &mut response)?;
                let cipher_suite = u32::from_le_bytes(response);
                listener.on_ssl(
                    ssl_res,
                    Some(cipher_suite),
                    #[cfg(feature = "experimental-ecc")]
                    None,
                );
            }
            _ => {
                error!("Received invalid SSL response: {:?}", ssl_res);
                return Err(Error::InvalidHifResponse("SSL"));
            }
        }

        Ok(())
    }

    /// Waits for new events and dispatches them to the provided event listener.
    ///
    /// # Arguments
    ///
    /// * `listener` - The event callback handler that will be invoked based on the event type.
    ///
    /// # Returns
    ///
    /// * `()` - If no error occurred while waiting for and processing new events.
    /// * `Error` - If an error occurred while waiting for or processing new events.
    pub fn dispatch_events_may_wait<T: EventListener>(
        &mut self,
        listener: &mut T,
    ) -> Result<(), Error> {
        #[cfg(feature = "irq")]
        self.chip.wait_for_interrupt();
        self.dispatch_events_new(listener)
    }

    /// Check for new events and dispatches them to the provided event listener.
    ///
    /// # Arguments
    ///
    /// * `listener` - The event callback handler that will be invoked based on the event type.
    ///
    /// # Returns
    ///
    /// * `()` - If no error occurred while checking or processing new events.
    /// * `Error` - If an error occurred while checking or processing new events.
    pub fn dispatch_events_new<T: EventListener>(&mut self, listener: &mut T) -> Result<(), Error> {
        // clear the interrupt pending register
        let res = self.is_interrupt_pending()?;
        if !res.0 {
            return Ok(());
        }
        self.clear_interrupt_pending(res.1)?;
        let (hif, _len, address) = self.read_hif_header(res.1)?;
        match hif {
            HifGroup::Wifi(e) => self.wifi_events_listener(listener, address, e),
            HifGroup::Ip(e) => self.ip_events_listener(listener, address, e),
            #[cfg(feature = "experimental-ota")]
            HifGroup::Ota(e) => self.ota_events_listener(listener, address, e),
            #[cfg(feature = "ssl")]
            HifGroup::Ssl(e) => self.ssl_events_listener(listener, address, e),
            _ => panic!("Unexpected hif"),
        }
    }
}
