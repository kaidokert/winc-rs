use crate::errors::CommError as Error;

use embedded_nal::nb;

use crate::manager::{
    AccessPoint, AuthType, Credentials, FirmwareInfo, HostName, IPConf, ProvisioningInfo,
    ScanResult, SocketOptions, Ssid, TcpSockOpts, UdpSockOpts, WifiChannel, WifiConnError,
};
#[cfg(feature = "ssl")]
use crate::manager::{SslSockConfig, SslSockOpts};

use crate::stack::{sock_holder::SocketStore, socket_callbacks::WifiModuleState};

use super::{Handle, PingResult, StackError, WincClient, Xfer};

use crate::{error, info};

// 1 minute max, if no other delays are added
const AP_CONNECT_TIMEOUT_MILLISECONDS: u32 = 60_000;
// 5 seconds max, assuming no additional delays
const AP_DISCONNECT_TIMEOUT_MILLISECONDS: u32 = 5_000;
// Timeout for Provisioning
#[cfg(not(test))]
const PROVISIONING_TIMEOUT: u32 = 60 * 1000;
#[cfg(test)]
const PROVISIONING_TIMEOUT: u32 = 1000;

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
                    let result = self.manager.boot_the_chip(state)?;
                    if result {
                        self.callbacks.state = WifiModuleState::Unconnected;
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

    /// Initializes the Wifi module in download mode to
    /// update firmware or download SSL certificates.
    ///
    /// # Returns
    ///
    /// * `()` - The Wi-Fi module has successfully started in download mode.
    /// * `nb::Error::WouldBlock` - The Wifi module is still starting.
    /// * `StackError` - An error occurred while starting the Wifi module.
    pub fn start_in_download_mode(&mut self) -> nb::Result<(), StackError> {
        match self.callbacks.state {
            WifiModuleState::Reset => {
                self.manager.set_crc_state(true);
                // wake-up the chip
                self.manager.chip_wake()?;
                // reset the chip
                self.manager.chip_reset()?;
                // halt the chip
                self.manager.chip_halt()?;
                self.callbacks.state = WifiModuleState::Starting;
                Err(nb::Error::WouldBlock)
            }

            WifiModuleState::Starting => {
                // set the spi packet size
                self.manager.configure_spi_packetsize()?;
                // read the chip id
                let chip_id = self.manager.chip_id()?;
                let chip_rev = self.manager.chip_rev()?;
                // disable all internal interrupts
                self.manager.disable_internal_interrupt()?;
                // enable the chip interrupts
                self.manager.enable_interrupt_pins()?;
                info!(
                    "Chip id: {:x} rev: {:x} booted into download mode.",
                    chip_id, chip_rev
                );
                self.callbacks.state = WifiModuleState::DownloadMode;
                Ok(())
            }

            WifiModuleState::DownloadMode => {
                info!("Chip is already in download mode.");
                Ok(())
            }

            _ => Err(nb::Error::Other(StackError::InvalidState)),
        }
    }

    fn connect_to_ap_impl(
        &mut self,
        connect_fn: impl FnOnce(&mut Self) -> Result<(), crate::errors::CommError>,
    ) -> nb::Result<(), StackError> {
        match self.callbacks.state {
            WifiModuleState::Unconnected => {
                self.operation_countdown = AP_CONNECT_TIMEOUT_MILLISECONDS;
                self.callbacks.state = WifiModuleState::ConnectingToAp;
                connect_fn(self)?;
                Err(nb::Error::WouldBlock)
            }
            WifiModuleState::ConnectingToAp => {
                self.delay_us(self.poll_loop_delay_us); // absolute minimum delay to make timeout possible
                self.dispatch_events_may_wait()?;
                self.operation_countdown -= 1;
                if self.operation_countdown == 0 {
                    return Err(nb::Error::Other(StackError::GeneralTimeout));
                }
                Err(nb::Error::WouldBlock)
            }
            WifiModuleState::ConnectionFailed => {
                // Change the state to `Unconnected` so that the client can make subsequent connection requests.
                self.callbacks.state = WifiModuleState::Unconnected;
                Err(nb::Error::Other(StackError::ApJoinFailed(
                    self.callbacks
                        .connection_state
                        .conn_error
                        .take()
                        .unwrap_or(WifiConnError::Unhandled),
                )))
            }
            WifiModuleState::ConnectedToAp => {
                info!("connect_to_ap: got Connected to AP");
                Ok(())
            }
            _ => Err(nb::Error::Other(StackError::InvalidState)),
        }
    }

    /// Connect to access point with previously saved credentials
    pub fn connect_to_saved_ap(&mut self) -> nb::Result<(), StackError> {
        self.connect_to_ap_impl(|inner_self: &mut Self| inner_self.manager.send_default_connect())
    }

    /// Connect to access point with given SSID and credentials.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID of the access point to connect to.
    /// * `credentials` - Security credentials (e.g., passphrase or authentication data).
    /// * `channel` - Wi-Fi RF channel.
    /// * `save_credentials` - Whether to save the credentials to the module.
    ///
    /// # Returns
    ///
    /// * `()` - Successfully connected to access point.
    /// * `StackError` - If an error occurs while connecting with access point.
    pub fn connect_to_ap(
        &mut self,
        ssid: &Ssid,
        credentials: &Credentials,
        channel: WifiChannel,
        save_credentials: bool,
    ) -> nb::Result<(), StackError> {
        self.connect_to_ap_impl(|inner_self: &mut Self| {
            inner_self
                .manager
                .send_connect(ssid, credentials, channel, !save_credentials)
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
                self.manager.send_scan(0xFF, PASSIVE_SCAN_TIME)?;
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

        self.dispatch_events_may_wait()?;
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
                self.manager.send_get_scan_result(index)?;
                self.callbacks.connection_state.scan_results = Some(None);
            }
            Some(result) => {
                if let Some(result) = result.take() {
                    self.callbacks.connection_state.scan_results = None;
                    return Ok(result);
                }
            }
        }

        self.dispatch_events_may_wait()?;
        Err(nb::Error::WouldBlock)
    }

    pub fn get_ip_settings(&mut self) -> nb::Result<IPConf, StackError> {
        if let Some(ip_conf) = &self.callbacks.connection_state.ip_conf {
            return Ok(ip_conf.clone());
        }

        self.dispatch_events_may_wait()?;
        Err(nb::Error::WouldBlock)
    }

    /// Gets current RSSI level
    pub fn get_current_rssi(&mut self) -> nb::Result<i8, StackError> {
        match &mut self.callbacks.connection_state.rssi_level {
            None => {
                self.manager.send_get_current_rssi()?;
                self.callbacks.connection_state.rssi_level = Some(None);
            }
            Some(rssi) => {
                if let Some(rssi) = rssi.take() {
                    self.callbacks.connection_state.rssi_level = None;
                    return Ok(rssi);
                }
            }
        }

        self.dispatch_events_may_wait()?;
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
                self.manager.send_get_conn_info()?;
                self.callbacks.connection_state.conn_info = Some(None);
            }
            Some(info) => {
                if let Some(info) = info.take() {
                    self.callbacks.connection_state.conn_info = None;
                    return Ok(info);
                }
            }
        }
        self.dispatch_events_may_wait()?;
        Err(nb::Error::WouldBlock)
    }

    /// Get the firmware version of the Wifi module
    pub fn get_firmware_version(&mut self) -> Result<FirmwareInfo, StackError> {
        Ok(self.manager.get_firmware_ver_full()?)
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
                self.manager.send_ping_req(dest_ip, ttl, count, marker)?;
                self.callbacks.connection_state.ping_result = Some(None);
            }
            Some(result) => {
                if let Some(result) = result.take() {
                    self.callbacks.connection_state.ping_result = None;
                    return Ok(result);
                }
            }
        }

        self.dispatch_events_may_wait()?;
        Err(nb::Error::WouldBlock)
    }

    /// Sends a disconnect request to the currently connected AP.
    ///
    /// This command is only applicable in station mode.
    pub fn disconnect_ap(&mut self) -> nb::Result<(), StackError> {
        match &mut self.callbacks.state {
            WifiModuleState::ConnectedToAp => {
                self.operation_countdown = AP_DISCONNECT_TIMEOUT_MILLISECONDS;
                self.callbacks.state = WifiModuleState::Disconnecting;
                self.manager.send_disconnect()?;
                Err(nb::Error::WouldBlock)
            }
            WifiModuleState::Disconnecting => {
                self.delay_us(self.poll_loop_delay_us); // absolute minimum delay to make timeout possible
                self.dispatch_events_may_wait()?;
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

    /// Starts the provisioning mode. This command is only applicable when the chip is in station mode.
    ///
    /// # Arguments
    ///
    /// * `ap` - An `AccessPoint` struct containing the SSID, password, and other network details.
    /// * `hostname` - Device domain name. Must not include `.local`.
    /// * `http_redirect` - Whether HTTP redirection is enabled.
    /// * `timeout` - The timeout duration for provisioning, in minutes.
    ///
    /// # Returns
    ///
    /// * `ProvisioningInfo` - Wifi Credentials received from provisioning.
    /// * `StackError` - If an error occurs while starting provisioning mode or receiving provisioning information.
    pub fn provisioning_mode(
        &mut self,
        ap: &AccessPoint,
        hostname: &HostName,
        http_redirect: bool,
        timeout: u32,
    ) -> nb::Result<ProvisioningInfo, StackError> {
        match &mut self.callbacks.state {
            WifiModuleState::Unconnected | WifiModuleState::ConnectedToAp => {
                let auth = <Credentials as Into<AuthType>>::into(ap.key);

                if auth == AuthType::S802_1X {
                    error!("Enterprise Security in provisioning mode is not supported");
                    return Err(nb::Error::Other(StackError::InvalidParameters));
                }

                self.manager
                    .send_start_provisioning(ap, hostname, http_redirect)?;

                self.callbacks.state = WifiModuleState::Provisioning;
                self.callbacks.provisioning_info = None;
            }
            WifiModuleState::Provisioning => match &mut self.callbacks.provisioning_info {
                None => {
                    self.operation_countdown = timeout * PROVISIONING_TIMEOUT;
                    self.callbacks.provisioning_info = Some(None);
                }
                Some(result) => {
                    if let Some(info) = result.take() {
                        if info.status {
                            return Ok(info);
                        }
                        return Err(nb::Error::Other(StackError::WincWifiFail(Error::Failed)));
                    } else {
                        self.delay_us(self.poll_loop_delay_us);
                        self.operation_countdown -= 1;
                        if self.operation_countdown == 0 {
                            return Err(nb::Error::Other(StackError::GeneralTimeout));
                        }
                    }
                }
            },
            _ => {
                return Err(nb::Error::Other(StackError::InvalidState));
            }
        }

        self.dispatch_events_may_wait()?;
        Err(nb::Error::WouldBlock)
    }

    /// Stops provisioning mode. This command is only applicable when the chip is in provisioning mode.
    ///
    /// # Returns
    ///
    /// * `()` - If provisioning mode starts successfully.
    /// * `StackError` - If an error occurs while stopping provisioning mode.
    pub fn stop_provisioning_mode(&mut self) -> Result<(), StackError> {
        if self.callbacks.state == WifiModuleState::Provisioning {
            self.manager.send_stop_provisioning()?;
        } else {
            return Err(StackError::InvalidState);
        }

        // change the state to unconnected
        self.callbacks.state = WifiModuleState::Unconnected;
        Ok(())
    }

    /// Enable the Access Point mode.
    ///
    /// # Arguments
    ///
    /// * `ap` - An `AccessPoint` struct containing the SSID, password, and other network details.
    ///
    /// # Returns
    ///
    /// * `()` - Access point mode is successfully enabled.
    /// * `StackError` - If an error occurs while enabling access point mode.
    pub fn enable_access_point(&mut self, ap: &AccessPoint) -> Result<(), StackError> {
        if self.callbacks.state == WifiModuleState::Unconnected {
            let auth: AuthType = ap.key.into();
            if auth == AuthType::S802_1X {
                error!("Enterprise Security is not supported in access point mode");
                return Err(StackError::InvalidParameters);
            }
            self.manager.send_enable_access_point(ap)?;
            self.callbacks.state = WifiModuleState::AccessPoint;
        } else {
            return Err(StackError::InvalidState);
        }

        Ok(())
    }

    /// Disable the Access Point mode.
    ///
    /// # Returns
    ///
    /// * `()` - Access point mode is successfully disabled.
    /// * `StackError` - If an error occurs while disabling access point mode.
    pub fn disable_access_point(&mut self) -> Result<(), StackError> {
        if self.callbacks.state == WifiModuleState::AccessPoint {
            self.manager.send_disable_access_point()?;
            self.callbacks.state = WifiModuleState::Unconnected;
        } else {
            return Err(StackError::InvalidState);
        }

        Ok(())
    }

    /// Sets the specified socket option on the given socket.
    ///
    /// # Arguments
    ///
    /// * `socket` - A socket handle to configure.
    /// * `option` - The socket option to apply.
    ///
    /// # Returns
    ///
    /// * `()` - If the socket option was successfully applied.
    /// * `StackError` - If an error occurs while applying the socket option.
    pub fn set_socket_option(
        &mut self,
        socket: &Handle,
        option: &SocketOptions,
    ) -> Result<(), StackError> {
        match option {
            SocketOptions::Udp(opts) => {
                let (sock, _) = self
                    .callbacks
                    .udp_sockets
                    .get(*socket)
                    .ok_or(StackError::SocketNotFound)?;

                if let UdpSockOpts::ReceiveTimeout(timeout) = opts {
                    // Receive timeout are handled by winc stack not by module.
                    sock.set_recv_timeout(*timeout);
                } else {
                    self.manager.send_setsockopt(*sock, opts)?;
                }
            }

            SocketOptions::Tcp(opts) => {
                let (sock, _) = self
                    .callbacks
                    .tcp_sockets
                    .get(*socket)
                    .ok_or(StackError::SocketNotFound)?;

                match opts {
                    #[cfg(feature = "ssl")]
                    TcpSockOpts::Ssl(ssl_opts) => {
                        match *ssl_opts {
                            SslSockOpts::SetSni(_) => {
                                self.manager.send_ssl_setsockopt(*sock, ssl_opts)?;
                            }
                            SslSockOpts::Config(cfg, en) => {
                                if cfg == SslSockConfig::EnableSSL && en {
                                    self.manager.send_ssl_sock_create(*sock)?;
                                }
                                // Set the SSL flags
                                sock.set_ssl_cfg(cfg.into(), en);
                            }
                        }
                    }
                    TcpSockOpts::ReceiveTimeout(timeout) => {
                        // Receive timeout are handled by winc stack not by module.
                        sock.set_recv_timeout(*timeout);
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use core::net::Ipv4Addr;

    use super::*;
    use crate::client::{test_shared::*, SocketCallbacks};
    //use crate::manager::Error::BootRomStart;
    use crate::errors::CommError as Error;
    use crate::manager::Ssid;
    use crate::manager::{EventListener, PingError, WifiConnError, WifiConnState};
    use crate::{ConnectionInfo, Credentials, S8Password, S8Username, WifiChannel, WpaKey};
    #[cfg(feature = "wep")]
    use crate::{WepKey, WepKeyIndex};
    use embedded_nal::{TcpClientStack, UdpClientStack};

    #[cfg(feature = "ssl")]
    use crate::manager::SslSockConfig;

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
        client.callbacks.state = WifiModuleState::Unconnected;
        let result = nb::block!(client.connect_to_saved_ap());
        assert_eq!(result, Err(StackError::GeneralTimeout));
    }
    #[test]
    fn test_connect_to_saved_ap_success() {
        let mut client = make_test_client();
        client.callbacks.state = WifiModuleState::Unconnected;
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
        client.callbacks.state = WifiModuleState::Unconnected;
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
        let ssid = Ssid::from("test").unwrap();
        let key = Credentials::WpaPSK(WpaKey::from("test").unwrap());
        client.callbacks.state = WifiModuleState::Unconnected;
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connstate_changed(WifiConnState::Connected, WifiConnError::Unhandled);
        };
        client.debug_callback = Some(&mut my_debug);
        let result = nb::block!(client.connect_to_ap(&ssid, &key, WifiChannel::Channel1, false));
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_scan_ok() {
        let mut client = make_test_client();
        client.callbacks.state = WifiModuleState::Unconnected;
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_scan_done(5, WifiConnError::NoError);
        };
        client.debug_callback = Some(&mut my_debug);
        let result = nb::block!(client.scan());
        assert_eq!(result, Ok(5));
    }

    #[test]
    fn test_get_scan_result_ok() {
        let mut client = make_test_client();
        client.callbacks.state = WifiModuleState::Unconnected;
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
        client.callbacks.state = WifiModuleState::Unconnected;
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
        client.callbacks.state = WifiModuleState::Unconnected;
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
        client.callbacks.state = WifiModuleState::Unconnected;
        let result = client.get_firmware_version();
        assert_eq!(result.unwrap().chip_id, 0);
    }

    #[test]
    fn test_send_ping_ok() {
        let mut client = make_test_client();
        client.callbacks.state = WifiModuleState::Unconnected;
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

    #[test]
    fn test_provisioning_mode_open_success() {
        // test client
        let mut client = make_test_client();
        // ssid for access point configuration.
        let ap_ssid = Ssid::from("ssid").unwrap();
        // access point configuration.
        let ap = AccessPoint::open(&ap_ssid);
        // hostname for access point.
        let hostname = HostName::from("admin").unwrap();
        // ssid received from provisioning.
        let test_ssid = Ssid::from("test_ssid").unwrap();
        // Wpa key passed to provisioning callback.
        // Should be empty for Open network.
        let test_key = WpaKey::new();
        // debug callback
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_provisioning(test_ssid, test_key, AuthType::Open, true);
        };

        client.debug_callback = Some(&mut my_debug);
        // set the module state to unconnected.
        client.callbacks.state = WifiModuleState::Unconnected;

        let result = nb::block!(client.provisioning_mode(&ap, &hostname, false, 1));

        assert!(result.is_ok());
        if let Ok(info) = result {
            assert_eq!(info.key, Credentials::Open);
            assert_eq!(info.ssid, test_ssid);
        } else {
            assert!(false);
        }
    }

    #[test]
    fn test_provisioning_mode_wpa_success() {
        // test client
        let mut client = make_test_client();
        // ssid for access point configuration.
        let ap_ssid = Ssid::from("ssid").unwrap();
        // wpa key for access point configuration.
        let ap_key = WpaKey::from("wpa_key").unwrap();
        // Access Point Configuration.
        let ap = AccessPoint::wpa(&ap_ssid, &ap_key);
        // hostname for access point.
        let hostname = HostName::from("admin").unwrap();
        // ssid received from provisioning.
        let test_ssid = Ssid::from("test_ssid").unwrap();
        // Wpa key passed to provisioning callback.
        let test_key = WpaKey::from("test_key").unwrap();
        // debug callback
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_provisioning(test_ssid, test_key, AuthType::WpaPSK, true);
        };

        client.debug_callback = Some(&mut my_debug);
        // set the module state to unconnected.
        client.callbacks.state = WifiModuleState::Unconnected;

        let result = nb::block!(client.provisioning_mode(&ap, &hostname, false, 1));

        assert!(result.is_ok());
        if let Ok(info) = result {
            assert_eq!(info.key, Credentials::WpaPSK(test_key));
            assert_eq!(info.ssid, test_ssid);
        } else {
            assert!(false);
        }
    }

    #[cfg(feature = "wep")]
    #[test]
    fn test_provisioning_mode_wep_success() {
        // test client
        let mut client = make_test_client();
        // ssid for access point configuration.
        let ap_ssid = Ssid::from("ssid").unwrap();
        // wep key for access point configuration.
        let ap_key = WepKey::from("wep_key").unwrap();
        // Wep key index
        let wep_key_index = WepKeyIndex::Key1;
        // Access Point Configuration.
        let ap = AccessPoint::wep(&ap_ssid, &ap_key, wep_key_index);
        // hostname for access point.
        let hostname = HostName::from("admin").unwrap();
        // ssid received from provisioning.
        let test_ssid = Ssid::from("test_ssid").unwrap();
        // Wpa key passed to provisioning callback.
        let test_key = WpaKey::from("test_wep_key").unwrap();
        // Wep Key received from provisioning.
        let test_wep_key = WepKey::from("test_wep_key").unwrap();
        // debug callback
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_provisioning(test_ssid, test_key, AuthType::WEP, true);
        };

        client.debug_callback = Some(&mut my_debug);
        // set the module state to unconnected.
        client.callbacks.state = WifiModuleState::Unconnected;

        let result = nb::block!(client.provisioning_mode(&ap, &hostname, false, 1));

        assert!(result.is_ok());
        if let Ok(info) = result {
            assert_eq!(info.key, Credentials::Wep(test_wep_key, wep_key_index));
            assert_eq!(info.ssid, test_ssid);
        } else {
            assert!(false);
        }
    }

    #[test]
    fn test_provisioning_mode_enterprise_fail() {
        // test client
        let mut client = make_test_client();
        // ssid for access point configuration.
        let ap_ssid = Ssid::from("ssid").unwrap();
        // S802_1X Username for network credentials.
        let s8_username = S8Username::from("username").unwrap();
        // S802_1X Password for network credentials.
        let s8_password = S8Password::from("password").unwrap();
        // S802_1X network credentials.
        let ap_key = Credentials::S802_1X(s8_username, s8_password);

        // Access Point Configuration.
        let ap = AccessPoint {
            ssid: &ap_ssid,
            key: ap_key,
            channel: WifiChannel::Channel1,
            ssid_hidden: false,
            ip: Ipv4Addr::new(192, 168, 1, 1),
        };

        // hostname for access point.
        let hostname = HostName::from("admin").unwrap();

        // set the module state to unconnected.
        client.callbacks.state = WifiModuleState::Unconnected;

        let result = nb::block!(client.provisioning_mode(&ap, &hostname, false, 1));

        assert!(result.is_err());
        if let Err(error) = result {
            assert_eq!(error, StackError::InvalidParameters);
        } else {
            assert!(false);
        }
    }

    #[test]
    fn test_provisioning_invalid_state() {
        // test client
        let mut client = make_test_client();
        // ssid for access point configuration.
        let ap_ssid = Ssid::from("ssid").unwrap();
        // access point configuration.
        let ap = AccessPoint::open(&ap_ssid);
        // hostname for access point.
        let hostname = HostName::from("admin").unwrap();

        // set the module state to unconnected.
        client.callbacks.state = WifiModuleState::ConnectingToAp;

        let result = nb::block!(client.provisioning_mode(&ap, &hostname, false, 1));

        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err, StackError::InvalidState);
        } else {
            assert!(false);
        }
    }

    #[test]
    fn test_provisioning_timeout() {
        // test client
        let mut client = make_test_client();
        // ssid for access point configuration.
        let ap_ssid = Ssid::from("ssid").unwrap();
        // access point configuration.
        let ap = AccessPoint::open(&ap_ssid);
        // hostname for access point.
        let hostname = HostName::from("admin").unwrap();

        // set the module state to unconnected.
        client.callbacks.state = WifiModuleState::Unconnected;

        let result = nb::block!(client.provisioning_mode(&ap, &hostname, false, 1500)); // Time is in milliseconds

        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err, StackError::GeneralTimeout);
        } else {
            assert!(false);
        }
    }

    #[test]
    fn test_provisioning_failed() {
        // test client
        let mut client = make_test_client();
        // ssid for access point configuration.
        let ap_ssid = Ssid::from("ssid").unwrap();
        // access point configuration.
        let ap = AccessPoint::open(&ap_ssid);
        // hostname for access point.
        let hostname = HostName::from("admin").unwrap();
        // ssid received from provisioning.
        let test_ssid = Ssid::from("test_ssid").unwrap();
        // Wpa key passed to provisioning callback.
        // Should be empty for Open network.
        let test_key = WpaKey::new();
        // debug callback
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_provisioning(test_ssid, test_key, AuthType::Open, false);
        };

        client.debug_callback = Some(&mut my_debug);
        // set the module state to unconnected.
        client.callbacks.state = WifiModuleState::Unconnected;

        let result = nb::block!(client.provisioning_mode(&ap, &hostname, false, 1));

        assert!(result.is_err());
        if let Err(error) = result {
            assert_eq!(error, StackError::WincWifiFail(Error::Failed));
        } else {
            assert!(false);
        }
    }

    #[test]
    fn test_stop_provisioning_success() {
        // test client
        let mut client = make_test_client();
        // set the module state to unconnected.
        client.callbacks.state = WifiModuleState::Provisioning;

        let result = client.stop_provisioning_mode();

        assert!(result.is_ok());
    }

    #[test]
    fn test_stop_provisioning_state_error() {
        // test client
        let mut client = make_test_client();
        // set the module state to unconnected.
        client.callbacks.state = WifiModuleState::Unconnected;

        let result = client.stop_provisioning_mode();

        assert!(result.is_err());
    }

    #[test]
    fn test_enable_access_point_success() {
        // test client
        let mut client = make_test_client();
        // set the module state to unconnected.
        client.callbacks.state = WifiModuleState::Unconnected;
        let ssid = Ssid::from("ssid").unwrap();
        let ap = AccessPoint::open(&ssid);
        let result = client.enable_access_point(&ap);

        assert!(result.is_ok());
        assert_eq!(client.callbacks.state, WifiModuleState::AccessPoint);
    }

    #[test]
    fn test_enable_access_point_invalid_security() {
        // test client
        let mut client = make_test_client();
        // set the module state to unconnected.
        client.callbacks.state = WifiModuleState::Unconnected;
        let ssid = Ssid::from("ssid").unwrap();
        let key = Credentials::from_s802("username", "password").unwrap();
        let ap = AccessPoint {
            ssid: &ssid,
            key: key,
            channel: WifiChannel::Channel1,
            ssid_hidden: false,
            ip: Ipv4Addr::new(192, 168, 1, 1),
        };
        let result = client.enable_access_point(&ap);

        assert_eq!(result.err(), Some(StackError::InvalidParameters));
    }

    #[test]
    fn test_enable_access_point_invalid_state() {
        // test client
        let mut client = make_test_client();
        // set the module state to unconnected.
        client.callbacks.state = WifiModuleState::Provisioning;
        let ssid = Ssid::from("ssid").unwrap();
        let ap = AccessPoint::open(&ssid);
        let result = client.enable_access_point(&ap);

        assert_eq!(result.err(), Some(StackError::InvalidState));
    }

    #[test]
    fn test_disable_access_point_success() {
        // test client
        let mut client = make_test_client();
        // set the module state to unconnected.
        client.callbacks.state = WifiModuleState::AccessPoint;

        let result = client.disable_access_point();

        assert!(result.is_ok());
    }

    #[test]
    fn test_disable_access_point_fail() {
        // test client
        let mut client = make_test_client();
        // set the module state to unconnected.
        client.callbacks.state = WifiModuleState::Unconnected;

        let result = client.disable_access_point();

        assert_eq!(result.err(), Some(StackError::InvalidState));
    }

    #[test]
    fn test_udp_sock_opt_multicast() {
        let mut client = make_test_client();
        let socket = UdpClientStack::socket(&mut client).unwrap();
        let addr = Ipv4Addr::new(192, 168, 1, 1);

        let option = SocketOptions::join_multicast_v4(addr);

        let result = client.set_socket_option(&socket, &option);

        assert!(result.is_ok());
    }

    #[test]
    fn test_udp_sock_opt_invalid_socket() {
        let mut client = make_test_client();
        let socket = TcpClientStack::socket(&mut client).unwrap();
        let addr = Ipv4Addr::new(192, 168, 1, 1);

        let option = SocketOptions::join_multicast_v4(addr);

        let result = client.set_socket_option(&socket, &option);

        assert_eq!(result.err(), Some(StackError::SocketNotFound));
    }

    #[test]
    fn test_tcp_sock_opt_invalid_socket() {
        let mut client = make_test_client();
        let socket = UdpClientStack::socket(&mut client).unwrap();

        let option = SocketOptions::set_tcp_receive_timeout(1500);

        let result = client.set_socket_option(&socket, &option);

        assert_eq!(result.err(), Some(StackError::SocketNotFound));
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_tcp_sock_opt_set_sni() {
        let mut client = make_test_client();
        let socket = TcpClientStack::socket(&mut client).unwrap();

        let option = SocketOptions::set_sni("hostname").unwrap();

        let result = client.set_socket_option(&socket, &option);

        assert!(result.is_ok());
    }

    #[test]
    fn test_udp_set_socket_timeout() {
        let mut client = make_test_client();
        let timeout = 1500 as u32;
        let socket = UdpClientStack::socket(&mut client).unwrap();

        let options = SocketOptions::set_udp_receive_timeout(timeout);

        let result = client.set_socket_option(&socket, &options);

        assert!(result.is_ok());

        let (sock, _) = client.callbacks.udp_sockets.get(socket).unwrap();

        assert_eq!(sock.get_recv_timeout(), timeout);
    }

    #[test]
    fn test_tcp_set_socket_timeout() {
        let mut client = make_test_client();
        let timeout = 150000 as u32;
        let socket = TcpClientStack::socket(&mut client).unwrap();

        let options = SocketOptions::set_tcp_receive_timeout(timeout);

        let result = client.set_socket_option(&socket, &options);

        assert!(result.is_ok());

        let (sock, _) = client.callbacks.tcp_sockets.get(socket).unwrap();

        assert_eq!(sock.get_recv_timeout(), timeout);
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_tcp_ssl_cfg() {
        let mut client = make_test_client();

        let ssl_opt = SocketOptions::config_ssl(SslSockConfig::EnableSSL, true);
        let socket = TcpClientStack::socket(&mut client).unwrap();

        let result = client.set_socket_option(&socket, &ssl_opt);

        assert!(result.is_ok());

        let (sock, _) = client.callbacks.tcp_sockets.get(socket).unwrap();

        assert_eq!(sock.get_ssl_cfg(), u8::from(SslSockConfig::EnableSSL));
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_tcp_ssl_cfg_disable() {
        let mut client = make_test_client();
        let socket = TcpClientStack::socket(&mut client).unwrap();

        // Enable first SSL config.
        let ssl_opt = SocketOptions::config_ssl(SslSockConfig::EnableSSL, true);
        let result = client.set_socket_option(&socket, &ssl_opt);
        assert!(result.is_ok());

        // Enable second config
        let ssl_opt = SocketOptions::config_ssl(SslSockConfig::EnableSniValidation, true);
        let result = client.set_socket_option(&socket, &ssl_opt);
        assert!(result.is_ok());

        // check the combined value
        {
            let (sock, _) = client.callbacks.tcp_sockets.get(socket).unwrap();

            assert_eq!(
                sock.get_ssl_cfg(),
                (u8::from(SslSockConfig::EnableSSL))
                    | (u8::from(SslSockConfig::EnableSniValidation))
            );
        }

        // Disable the first one
        let ssl_opt = SocketOptions::config_ssl(SslSockConfig::EnableSSL, false);
        let result = client.set_socket_option(&socket, &ssl_opt);
        assert!(result.is_ok());

        // check if first value is disabled
        let (sock, _) = client.callbacks.tcp_sockets.get(socket).unwrap();
        assert_eq!(
            sock.get_ssl_cfg(),
            u8::from(SslSockConfig::EnableSniValidation)
        );
    }
}
