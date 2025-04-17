use embedded_nal::nb;

use crate::manager::{AuthType, FirmwareInfo, IPConf, ScanResult};

use super::PingResult;
use super::StackError;
use super::WincClient;
use super::Xfer;

use crate::stack::socket_callbacks::WifiModuleState;

use crate::info;

// 1 minute max, if no other delays are added
const AP_CONNECT_TIMEOUT_MILLISECONDS: u32 = 60_000;
// 5 seconds max, assuming no additional delays
const AP_DISCONNECT_TIMEOUT_MILLISECONDS: u32 = 5_000;

impl<X: Xfer> WincClient<'_, X> {
    /// Call this periodically to receive network events
    ///
    /// Polls the chip for any events and changes in state,
    /// such as socket disconnects etc. This is internally
    /// called by other socket functions as well.
    pub fn heartbeat(&mut self) -> Result<(), StackError> {
        self.dispatch_events()?;
        Ok(())
    }

    /// Initializes the Wifi module - boots the firmware and
    /// does the rest of the initialization.
    ///
    /// # Returns
    ///
    /// * `()` - The Wifi module has been started.
    /// * `nb::Error::WouldBlock` - The Wifi module is still starting.
    /// * `StackError` - An error occurred while starting the Wifi module.
    pub fn start_wifi_module(&mut self) -> nb::Result<(), StackError> {
        match self.callbacks.state {
            WifiModuleState::Reset => {
                self.callbacks.state = WifiModuleState::Starting;
                self.manager.set_crc_state(true);
                self.boot = Some(Default::default());
                Err(nb::Error::WouldBlock)
            }
            WifiModuleState::Starting => {
                if let Some(state) = self.boot.as_mut() {
                    let result = self
                        .manager
                        .boot_the_chip(state)
                        .map_err(|x| nb::Error::Other(StackError::WincWifiFail(x)))?;
                    if result {
                        self.callbacks.state = WifiModuleState::Started;
                        self.boot = None;
                        return Ok(());
                    }
                    Err(nb::Error::WouldBlock)
                } else {
                    Err(nb::Error::Other(StackError::InvalidState))
                }
            }
            _ => Err(nb::Error::Other(StackError::InvalidState)),
        }
    }

    fn connect_to_ap_impl(
        &mut self,
        connect_fn: impl FnOnce(&mut Self) -> Result<(), crate::errors::Error>,
    ) -> nb::Result<(), StackError> {
        match self.callbacks.state {
            WifiModuleState::Reset | WifiModuleState::Starting | WifiModuleState::Disconnecting => {
                Err(nb::Error::Other(StackError::InvalidState))
            }
            WifiModuleState::Started | WifiModuleState::Disconnected => {
                self.operation_countdown = AP_CONNECT_TIMEOUT_MILLISECONDS;
                self.callbacks.state = WifiModuleState::ConnectingToAp;
                connect_fn(self).map_err(|x| nb::Error::Other(StackError::WincWifiFail(x)))?;
                Err(nb::Error::WouldBlock)
            }
            WifiModuleState::ConnectingToAp => {
                self.delay_us(self.poll_loop_delay_us); // absolute minimum delay to make timeout possible
                self.dispatch_events()?;
                self.operation_countdown -= 1;
                if self.operation_countdown == 0 {
                    return Err(nb::Error::Other(StackError::GeneralTimeout));
                }
                Err(nb::Error::WouldBlock)
            }
            WifiModuleState::ConnectionFailed => Err(nb::Error::Other(StackError::ApJoinFailed(
                self.callbacks.connection_state.conn_error.take().unwrap(),
            ))),
            WifiModuleState::ConnectedToAp => {
                info!("connect_to_ap: got Connected to AP");
                Ok(())
            }
        }
    }

    /// Connect to access point with previously saved credentials
    pub fn connect_to_saved_ap(&mut self) -> nb::Result<(), StackError> {
        self.connect_to_ap_impl(|inner_self: &mut Self| inner_self.manager.send_default_connect())
    }

    /// Connect to access point with given SSID and password, with WPA2 security
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID of the access point to connect to.
    /// * `password` - The password of the access point to connect to.
    /// * `save_credentials` - Whether to save the credentials to the module.
    ///
    pub fn connect_to_ap(
        &mut self,
        ssid: &str,
        password: &str,
        save_credentials: bool,
    ) -> nb::Result<(), StackError> {
        self.connect_to_ap_impl(|inner_self: &mut Self| {
            inner_self.manager.send_connect(
                AuthType::WpaPSK,
                ssid,
                password,
                0xFF,
                !save_credentials,
            )
        })
    }

    /// Trigger a scan for available access points
    ///
    /// This is a non-blocking call, and takes a few seconds
    /// to complete.
    /// Results are kept in an internal buffer - retrieve
    /// them by index with [WincClient::get_scan_result]
    ///
    /// # Returns
    ///
    /// * `num_aps` - The number of access points found.
    ///
    pub fn scan(&mut self) -> nb::Result<u8, StackError> {
        match &mut self.callbacks.connection_state.scan_number_aps {
            None => {
                // This is ignored for active scan
                const PASSIVE_SCAN_TIME: u16 = 1000;
                self.manager
                    .send_scan(0xFF, PASSIVE_SCAN_TIME)
                    .map_err(|x| nb::Error::Other(StackError::WincWifiFail(x)))?;
                // Signal operation in progress
                self.callbacks.connection_state.scan_number_aps = Some(None);
            }
            Some(num_aps) => {
                if let Some(num_aps) = num_aps.take() {
                    if let Some(err) = self.callbacks.connection_state.conn_error.take() {
                        return Err(nb::Error::Other(StackError::ApScanFailed(err)));
                    }
                    self.callbacks.connection_state.scan_number_aps = None;
                    return Ok(num_aps);
                }
            }
        }

        self.dispatch_events()?;
        Err(nb::Error::WouldBlock)
    }

    /// Get the scan result for an access point
    ///
    /// # Arguments
    ///
    /// * `index` - The index of the access point to get the result for.
    ///
    /// # Returns
    ///
    /// * `ScanResult` - The scan result for the access point.
    ///
    pub fn get_scan_result(&mut self, index: u8) -> nb::Result<ScanResult, StackError> {
        match &mut self.callbacks.connection_state.scan_results {
            None => {
                self.manager
                    .send_get_scan_result(index)
                    .map_err(StackError::WincWifiFail)?;
                self.callbacks.connection_state.scan_results = Some(None);
            }
            Some(result) => {
                if let Some(result) = result.take() {
                    self.callbacks.connection_state.scan_results = None;
                    return Ok(result);
                }
            }
        }

        self.dispatch_events()?;
        Err(nb::Error::WouldBlock)
    }

    pub fn get_ip_settings(&mut self) -> nb::Result<IPConf, StackError> {
        if let Some(ip_conf) = &self.callbacks.connection_state.ip_conf {
            return Ok(ip_conf.clone());
        }

        self.dispatch_events()?;
        Err(nb::Error::WouldBlock)
    }

    /// Gets current RSSI level
    pub fn get_current_rssi(&mut self) -> nb::Result<i8, StackError> {
        match &mut self.callbacks.connection_state.rssi_level {
            None => {
                self.manager
                    .send_get_current_rssi()
                    .map_err(StackError::WincWifiFail)?;
                self.callbacks.connection_state.rssi_level = Some(None);
            }
            Some(rssi) => {
                if let Some(rssi) = rssi.take() {
                    self.callbacks.connection_state.rssi_level = None;
                    return Ok(rssi);
                }
            }
        }

        self.dispatch_events()?;
        Err(nb::Error::WouldBlock)
    }

    /// Gets current access point connection info
    ///
    /// # Returns
    ///
    /// * `ConnectionInfo` - The current connection info for the access point.
    ///
    ///
    pub fn get_connection_info(
        &mut self,
    ) -> nb::Result<crate::manager::ConnectionInfo, StackError> {
        match &mut self.callbacks.connection_state.conn_info {
            None => {
                self.manager
                    .send_get_conn_info()
                    .map_err(StackError::WincWifiFail)?;
                self.callbacks.connection_state.conn_info = Some(None);
            }
            Some(info) => {
                if let Some(info) = info.take() {
                    self.callbacks.connection_state.conn_info = None;
                    return Ok(info);
                }
            }
        }
        self.dispatch_events()?;
        Err(nb::Error::WouldBlock)
    }

    /// Get the firmware version of the Wifi module
    pub fn get_firmware_version(&mut self) -> Result<FirmwareInfo, StackError> {
        self.manager
            .get_firmware_ver_full()
            .map_err(StackError::WincWifiFail)
    }

    /// Sends a ping request to the given IP address
    ///
    /// # Arguments
    ///
    /// * `dest_ip` - The IP address to send the ping request to.
    /// * `ttl` - The time to live for the ping request.
    /// * `count` - The number of ping requests to send.
    ///
    /// # Returns
    ///
    /// * `PingResult` - The result of the ping request.
    ///
    pub fn send_ping(
        &mut self,
        dest_ip: core::net::Ipv4Addr,
        ttl: u8,
        count: u16,
    ) -> nb::Result<PingResult, StackError> {
        match &mut self.callbacks.connection_state.ping_result {
            None => {
                info!("sending ping request");
                let marker = 42; // This seems arbitrary pass through value
                self.manager
                    .send_ping_req(dest_ip, ttl, count, marker)
                    .map_err(StackError::WincWifiFail)?;
                self.callbacks.connection_state.ping_result = Some(None);
            }
            Some(result) => {
                if let Some(result) = result.take() {
                    self.callbacks.connection_state.ping_result = None;
                    return Ok(result);
                }
            }
        }

        self.dispatch_events()?;
        Err(nb::Error::WouldBlock)
    }

    /// Internal function for handling the disconnect request and response.
    fn disconnect_ap_impl(
        &mut self,
        disconnect_fn: impl FnOnce(&mut Self) -> Result<(), crate::errors::Error>,
    ) -> nb::Result<(), StackError> {
        match &mut self.callbacks.state {
            WifiModuleState::ConnectedToAp => {
                self.operation_countdown = AP_DISCONNECT_TIMEOUT_MILLISECONDS;
                self.callbacks.state = WifiModuleState::Disconnecting;
                disconnect_fn(self).map_err(|x| nb::Error::Other(StackError::WincWifiFail(x)))?;
                Err(nb::Error::WouldBlock)
            }
            WifiModuleState::Disconnecting => {
                self.delay_us(self.poll_loop_delay_us); // absolute minimum delay to make timeout possible
                self.dispatch_events()?;
                self.operation_countdown -= 1;
                if self.operation_countdown == 0 {
                    return Err(nb::Error::Other(StackError::GeneralTimeout));
                }
                Err(nb::Error::WouldBlock)
            }
            _ => {
                info!("disconnect_ap: got disconnected from AP");
                Ok(())
            }
        }
    }

    /// Sends a disconnect request to the currently connected AP.
    ///
    /// This command is only applicable in station mode.
    pub fn disconnect_ap(&mut self) -> nb::Result<(), StackError> {
        self.disconnect_ap_impl(|inner_self: &mut Self| inner_self.manager.send_disconnect())
    }
}

#[cfg(test)]
mod tests {
    use core::net::Ipv4Addr;

    use super::*;
    use crate::client::{test_shared::*, SocketCallbacks};
    //use crate::manager::Error::BootRomStart;
    use crate::errors::Error;
    use crate::manager::Ssid;
    use crate::manager::{EventListener, PingError, WifiConnError, WifiConnState};
    use crate::ConnectionInfo;

    #[test]
    fn test_heartbeat() {
        assert_eq!(make_test_client().heartbeat(), Ok(()));
    }

    #[test]
    fn test_start_wifi_module() {
        let mut client = make_test_client();
        let result = nb::block!(client.start_wifi_module());
        assert_eq!(
            result,
            Err(StackError::WincWifiFail(Error::BootRomStart).into())
        );
    }

    #[test]
    fn test_connect_to_saved_ap_invalid_state() {
        let mut client = make_test_client();
        let result = nb::block!(client.connect_to_saved_ap());
        assert_eq!(result, Err(StackError::InvalidState));
    }
    #[test]
    fn test_connect_to_saved_ap_timeout() {
        let mut client = make_test_client();
        client.callbacks.state = WifiModuleState::Started;
        let result = nb::block!(client.connect_to_saved_ap());
        assert_eq!(result, Err(StackError::GeneralTimeout));
    }
    #[test]
    fn test_connect_to_saved_ap_success() {
        let mut client = make_test_client();
        client.callbacks.state = WifiModuleState::Started;
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connstate_changed(WifiConnState::Connected, WifiConnError::Unhandled);
        };
        client.debug_callback = Some(&mut my_debug);
        let result = nb::block!(client.connect_to_saved_ap());
        assert_eq!(result, Ok(()));
    }
    #[test]
    fn test_connect_to_saved_ap_invalid_credentials() {
        let mut client = make_test_client();
        client.callbacks.state = WifiModuleState::Started;
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connstate_changed(WifiConnState::Disconnected, WifiConnError::AuthFail);
        };
        client.debug_callback = Some(&mut my_debug);
        let result = nb::block!(client.connect_to_saved_ap());
        assert_eq!(
            result,
            Err(StackError::ApJoinFailed(WifiConnError::AuthFail))
        );
    }

    #[test]
    fn test_connect_to_ap_success() {
        let mut client = make_test_client();
        client.callbacks.state = WifiModuleState::Started;
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connstate_changed(WifiConnState::Connected, WifiConnError::Unhandled);
        };
        client.debug_callback = Some(&mut my_debug);
        let result = nb::block!(client.connect_to_ap("test", "test", false));
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_scan_ok() {
        let mut client = make_test_client();
        client.callbacks.state = WifiModuleState::Started;
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_scan_done(5, WifiConnError::Unhandled);
        };
        client.debug_callback = Some(&mut my_debug);
        let result = nb::block!(client.scan());
        assert_eq!(result, Ok(5));
    }

    #[test]
    fn test_get_scan_result_ok() {
        let mut client = make_test_client();
        client.callbacks.state = WifiModuleState::Started;
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_scan_result(ScanResult {
                index: 0,
                rssi: 0,
                auth: AuthType::Open,
                channel: 0,
                bssid: [0; 6],
                ssid: Ssid::from("test").unwrap(),
            });
        };
        client.debug_callback = Some(&mut my_debug);
        let result = nb::block!(client.get_scan_result(0));
        assert_eq!(result.unwrap().ssid, Ssid::from("test").unwrap());
    }

    #[test]
    fn test_get_current_rssi_ok() {
        let mut client = make_test_client();
        client.callbacks.state = WifiModuleState::Started;
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_rssi(0);
        };
        client.debug_callback = Some(&mut my_debug);
        let result = nb::block!(client.get_current_rssi());
        assert_eq!(result, Ok(0));
    }

    #[test]
    fn test_get_connection_info_ok() {
        let mut client = make_test_client();
        client.callbacks.state = WifiModuleState::Started;
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connection_info(ConnectionInfo {
                ssid: Ssid::from("test").unwrap(),
                auth: AuthType::Open,
                ip: Ipv4Addr::new(192, 168, 1, 1),
                mac: [0; 6],
                rssi: 0,
            });
        };
        client.debug_callback = Some(&mut my_debug);
        let result = nb::block!(client.get_connection_info());
        assert_eq!(result.unwrap().ssid, Ssid::from("test").unwrap());
    }

    #[test]
    fn test_get_firmware_version_ok() {
        let mut client = make_test_client();
        client.callbacks.state = WifiModuleState::Started;
        let result = client.get_firmware_version();
        assert_eq!(result.unwrap().chip_id, 0);
    }

    #[test]
    fn test_send_ping_ok() {
        let mut client = make_test_client();
        client.callbacks.state = WifiModuleState::Started;
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_ping(
                Ipv4Addr::new(192, 168, 1, 1),
                0,
                42,
                0,
                0,
                PingError::Unhandled,
            );
        };
        client.debug_callback = Some(&mut my_debug);
        let result = nb::block!(client.send_ping(Ipv4Addr::new(192, 168, 1, 1), 64, 1));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().rtt, 42);
    }

    #[test]
    fn test_disconnect_success() {
        let mut client = make_test_client();
        client.callbacks.state = WifiModuleState::ConnectedToAp;

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connstate_changed(WifiConnState::Disconnected, WifiConnError::Unhandled);
        };

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.disconnect_ap());

        assert!(result.is_ok());
    }

    #[test]
    fn test_disconnect_timeout() {
        let mut client = make_test_client();
        client.callbacks.state = WifiModuleState::ConnectedToAp;

        let result = nb::block!(client.disconnect_ap());

        assert_eq!(result.err(), Some(StackError::GeneralTimeout));
    }

    #[test]
    fn test_disconnect_while_not_connected() {
        let mut client = make_test_client();
        client.callbacks.state = WifiModuleState::Starting;

        let result = nb::block!(client.disconnect_ap());

        assert!(result.is_ok());
    }
}
