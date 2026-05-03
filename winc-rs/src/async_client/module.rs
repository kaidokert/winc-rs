use super::{AsyncClient, Handle, StackError};
use crate::manager::{
    AccessPoint, BootMode, BootState, Credentials, FirmwareInfo, HostName, MacAddress,
    ProvisioningInfo, SocketOptions, Ssid, WifiChannel,
};
use crate::net_ops::module::{ProvisioningMode, StationMode, SyncOp};
use crate::transfer::Xfer;

impl<X: Xfer> AsyncClient<'_, X> {
    /// Initializes the WiFi module in normal mode.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the WiFi module starts successfully.
    /// * `Err(StackError)` - If an error occurs during initialization.
    pub async fn start_wifi_module(&mut self) -> Result<(), StackError> {
        let mut boot = BootState::new(BootMode::Normal);
        self.poll_op(&mut boot).await
    }

    /// Initializes the WiFi module in ethernet mode.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the WiFi module starts successfully.
    /// * `Err(StackError)` - If an error occurs during initialization.
    #[cfg(feature = "ethernet")]
    pub async fn start_in_ethernet_mode(&mut self) -> Result<(), StackError> {
        let mut boot = BootState::new(BootMode::Ethernet);
        self.poll_op(&mut boot).await
    }

    /// Initializes the Wifi module in download mode to update
    /// firmware or download SSL certificates.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - The Wi-Fi module has successfully started in download mode.
    /// * `Err(StackError)` - An error occurred while starting the Wifi module.
    #[cfg(feature = "flash-rw")]
    pub async fn start_in_download_mode(&mut self) -> Result<(), StackError> {
        let mut boot = BootState::new(BootMode::Download);
        self.poll_op(&mut boot).await
    }

    /// Connect to access point with previously saved credentials.
    pub async fn connect_to_saved_ap(&mut self) -> Result<(), StackError> {
        let mut op = StationMode::from_defaults();
        self.poll_op(&mut op).await
    }

    /// Connects to the access point with the given SSID and credentials.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID of the access point to connect to.
    /// * `credentials` - Security credentials (e.g., passphrase or authentication data).
    /// * `channel` - Wi-Fi RF channel (e.g., 1-14 or 255 to select all channels).
    /// * `save_credentials` - Whether to store the credentials persistently on the module.
    ///
    /// # Returns
    ///
    /// * `()` - Successfully connected to the access point.
    /// * `StackError` - If the connection to the access point fails.
    pub async fn connect_to_ap(
        &mut self,
        ssid: &Ssid,
        credentials: &Credentials,
        channel: WifiChannel,
        save_credentials: bool,
    ) -> Result<(), StackError> {
        let mut op = StationMode::from_credentials(ssid, credentials, channel, save_credentials);
        self.poll_op(&mut op).await
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
        let mut op = SyncOp::set_socket_options(socket, option);
        self.poll_once(&mut op)
    }

    /// Stops provisioning mode. This command is only applicable when the chip is in provisioning mode.
    ///
    /// # Returns
    ///
    /// * `()` - If provisioning mode starts successfully.
    /// * `StackError` - If an error occurs while stopping provisioning mode.
    pub fn stop_provisioning_mode(&mut self) -> Result<(), StackError> {
        let mut op = SyncOp::stop_provisioning_mode();
        self.poll_once(&mut op)
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
        let mut op = SyncOp::enable_access_point(ap);
        self.poll_once(&mut op)
    }

    /// Disable the Access Point mode.
    ///
    /// # Returns
    ///
    /// * `()` - Access point mode is successfully disabled.
    /// * `StackError` - If an error occurs while disabling access point mode.
    pub fn disable_access_point(&mut self) -> Result<(), StackError> {
        let mut op = SyncOp::disable_access_point();
        self.poll_once(&mut op)
    }

    /// Retrieves the MAC address from the WINC network interface.
    ///
    /// # Returns
    ///
    /// * `Ok(MacAddress)` - The current MAC address of the WINC module on success.
    /// * `Err(StackError)` - If the MAC address could not be retrieved.
    pub fn get_winc_mac_address(
        &mut self,
        #[cfg(test)] test_hook: bool,
    ) -> Result<MacAddress, StackError> {
        let mut op = SyncOp::get_winc_mac_address(
            #[cfg(test)]
            test_hook,
        );
        self.poll_once(&mut op)?;

        op.retrieve_winc_mac_address()
    }

    /// Retrieves the firmware version of the WiFi module.
    ///
    /// # Returns
    ///
    /// * `Ok(FirmwareInfo)` - The firmware version of the WINC module.
    /// * `Err(StackError)` - Returned if acquiring the firmware version fails.
    pub fn get_firmware_version(&mut self) -> Result<FirmwareInfo, StackError> {
        let mut op = SyncOp::get_firmware_version();
        self.poll_once(&mut op)?;

        op.retrieve_firmware_version()
    }

    /// Starts the provisioning mode. This command is only applicable when the chip is
    /// in station mode or unconnected.
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
    pub async fn start_provisioning_mode<'a>(
        &mut self,
        ap: &'a AccessPoint<'a>,
        hostname: &'a HostName,
        http_redirect: bool,
        timeout: u32,
    ) -> Result<ProvisioningInfo, StackError> {
        let mut op = ProvisioningMode::new(ap, hostname, http_redirect, timeout);
        self.poll_op(&mut op).await
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::make_test_client;
    use super::*;
    use crate::errors::CommError as Error;
    use crate::manager::{
        AuthType, EventListener, S8Password, S8Username, WifiConnError, WifiConnState, WpaKey,
    };
    use crate::stack::{
        sock_holder::SocketStore,
        socket_callbacks::{SocketCallbacks, WifiModuleState},
    };
    use core::net::Ipv4Addr;
    use macro_rules_attribute::apply;
    use smol_macros::test;

    #[cfg(feature = "ssl")]
    use crate::manager::SslSockConfig;

    #[cfg(feature = "wep")]
    use crate::{WepKey, WepKeyIndex};

    #[apply(test!)]
    async fn test_async_connect_to_saved_ap_invalid_state() {
        let mut client = make_test_client();
        let result = client.connect_to_saved_ap().await;
        assert_eq!(result, Err(StackError::InvalidState));
    }

    #[apply(test!)]
    async fn test_async_connect_to_saved_ap_timeout() {
        let result = {
            let mut client = make_test_client();
            client.callbacks.borrow_mut().state = WifiModuleState::Unconnected;
            client.connect_to_saved_ap().await
        };
        assert_eq!(result, Err(StackError::GeneralTimeout));
    }

    #[apply(test!)]
    async fn test_async_connect_to_saved_ap_invalid_credentials() {
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connstate_changed(WifiConnState::Disconnected, WifiConnError::AuthFail);
        };

        let result = {
            let mut client = make_test_client();
            client.callbacks.borrow_mut().state = WifiModuleState::Unconnected;
            *client.debug_callback.borrow_mut() = Some(&mut my_debug);
            client.connect_to_saved_ap().await
        };
        assert_eq!(
            result,
            Err(StackError::ApJoinFailed(WifiConnError::AuthFail))
        );
    }

    #[apply(test!)]
    async fn test_async_connect_to_ap_success() {
        let ssid = Ssid::from("test").unwrap();
        let key = Credentials::WpaPSK(WpaKey::from("test").unwrap());
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connstate_changed(WifiConnState::Connected, WifiConnError::Unhandled);
        };

        let result = {
            let mut client = make_test_client();
            client.callbacks.borrow_mut().state = WifiModuleState::Unconnected;
            *client.debug_callback.borrow_mut() = Some(&mut my_debug);
            client
                .connect_to_ap(&ssid, &key, WifiChannel::Channel1, false)
                .await
        };
        assert!(result.is_ok());
    }

    #[apply(test!)]
    async fn test_async_start_wifi_module_fail() {
        let mut client = make_test_client();
        let result = client.start_wifi_module().await;
        assert_eq!(
            result,
            Err(StackError::WincWifiFail(Error::BootRomStart).into())
        )
    }

    #[apply(test!)]
    async fn test_async_start_wifi_module_invalid_state() {
        let mut client = make_test_client();
        client.callbacks.borrow_mut().state = WifiModuleState::Unconnected;
        let result = client.start_wifi_module().await;
        assert_eq!(result, Err(StackError::InvalidState.into()))
    }

    #[cfg(feature = "flash-rw")]
    #[apply(test!)]
    async fn test_async_start_in_download_mode_fail() {
        let mut client = make_test_client();
        let result = client.start_in_download_mode().await;
        assert_eq!(
            result,
            Err(StackError::WincWifiFail(Error::OperationRetriesExceeded))
        );
    }

    #[cfg(feature = "flash-rw")]
    #[apply(test!)]
    async fn test_async_start_in_download_mode_invalid_state() {
        let mut client = make_test_client();
        client.callbacks.borrow_mut().state = WifiModuleState::Unconnected;
        let result = client.start_in_download_mode().await;
        assert_eq!(result, Err(StackError::InvalidState.into()))
    }

    #[test]
    fn test_async_stop_provisioning_success() {
        // test client
        let mut client = make_test_client();
        // set the module state to unconnected.
        client.callbacks.borrow_mut().state = WifiModuleState::Provisioning;

        let result = client.stop_provisioning_mode();

        assert!(result.is_ok());
    }

    #[test]
    fn test_async_stop_provisioning_state_error() {
        // test client
        let mut client = make_test_client();
        // set the module state to unconnected.
        client.callbacks.borrow_mut().state = WifiModuleState::Unconnected;

        let result = client.stop_provisioning_mode();

        assert!(result.is_err());
    }

    #[test]
    fn test_async_enable_access_point_success() {
        // test client
        let mut client = make_test_client();
        // set the module state to unconnected.
        client.callbacks.borrow_mut().state = WifiModuleState::Unconnected;
        let ssid = Ssid::from("ssid").unwrap();
        let ap = AccessPoint::open(&ssid);
        let result = client.enable_access_point(&ap);

        assert!(result.is_ok());
        assert_eq!(
            client.callbacks.borrow_mut().state,
            WifiModuleState::AccessPoint
        );
    }

    #[test]
    fn test_async_enable_access_point_invalid_security() {
        // test client
        let mut client = make_test_client();
        // set the module state to unconnected.
        client.callbacks.borrow_mut().state = WifiModuleState::Unconnected;
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
    fn test_async_enable_access_point_invalid_state() {
        // test client
        let mut client = make_test_client();
        // set the module state to unconnected.
        client.callbacks.borrow_mut().state = WifiModuleState::Provisioning;
        let ssid = Ssid::from("ssid").unwrap();
        let ap = AccessPoint::open(&ssid);
        let result = client.enable_access_point(&ap);

        assert_eq!(result.err(), Some(StackError::InvalidState));
    }

    #[test]
    fn test_async_disable_access_point_success() {
        // test client
        let mut client = make_test_client();
        // set the module state to unconnected.
        client.callbacks.borrow_mut().state = WifiModuleState::AccessPoint;

        let result = client.disable_access_point();

        assert!(result.is_ok());
    }

    #[test]
    fn test_async_disable_access_point_fail() {
        // test client
        let mut client = make_test_client();
        // set the module state to unconnected.
        client.callbacks.borrow_mut().state = WifiModuleState::Unconnected;

        let result = client.disable_access_point();

        assert_eq!(result.err(), Some(StackError::InvalidState));
    }

    #[test]
    fn test_async_udp_sock_opt_multicast() {
        let mut client = make_test_client();
        let socket = client.allocate_udp_socket().unwrap();
        let addr = Ipv4Addr::new(192, 168, 1, 1);

        let option = SocketOptions::join_multicast_v4(addr);

        let result = client.set_socket_option(&socket, &option);

        assert!(result.is_ok());
    }

    #[test]
    fn test_async_udp_sock_opt_invalid_socket() {
        let mut client = make_test_client();
        let socket = client.allocate_tcp_sockets().unwrap();
        let addr = Ipv4Addr::new(192, 168, 1, 1);

        let option = SocketOptions::join_multicast_v4(addr);

        let result = client.set_socket_option(&socket, &option);

        assert_eq!(result.err(), Some(StackError::SocketNotFound));
    }

    #[test]
    fn test_async_tcp_sock_opt_invalid_socket() {
        let mut client = make_test_client();
        let socket = client.allocate_udp_socket().unwrap();

        let option = SocketOptions::set_tcp_receive_timeout(1500);

        let result = client.set_socket_option(&socket, &option);

        assert_eq!(result.err(), Some(StackError::SocketNotFound));
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_async_tcp_sock_opt_set_sni() {
        let mut client = make_test_client();
        let socket = client.allocate_tcp_sockets().unwrap();

        let option = SocketOptions::set_sni("hostname").unwrap();

        let result = client.set_socket_option(&socket, &option);

        assert!(result.is_ok());
    }

    #[test]
    fn test_async_udp_set_socket_timeout() {
        let mut client = make_test_client();
        let timeout = 1500 as u32;
        let socket = client.allocate_udp_socket().unwrap();

        let options = SocketOptions::set_udp_receive_timeout(timeout);

        let result = client.set_socket_option(&socket, &options);

        assert!(result.is_ok());

        let (sock, _) = *client
            .callbacks
            .borrow_mut()
            .udp_sockets
            .get(socket)
            .unwrap();

        assert_eq!(sock.get_recv_timeout(), timeout);
    }

    #[test]
    fn test_async_tcp_set_socket_timeout() {
        let mut client = make_test_client();
        let timeout = 150000 as u32;
        let socket = client.allocate_tcp_sockets().unwrap();

        let options = SocketOptions::set_tcp_receive_timeout(timeout);

        let result = client.set_socket_option(&socket, &options);

        assert!(result.is_ok());

        let (sock, _) = *client
            .callbacks
            .borrow_mut()
            .tcp_sockets
            .get(socket)
            .unwrap();

        assert_eq!(sock.get_recv_timeout(), timeout);
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_async_tcp_ssl_cfg() {
        let mut client = make_test_client();

        let ssl_opt = SocketOptions::config_ssl(SslSockConfig::EnableSSL, true);
        let socket = client.allocate_tcp_sockets().unwrap();

        let result = client.set_socket_option(&socket, &ssl_opt);

        assert!(result.is_ok());

        let (sock, _) = *client
            .callbacks
            .borrow_mut()
            .tcp_sockets
            .get(socket)
            .unwrap();

        assert_eq!(sock.get_ssl_cfg(), u8::from(SslSockConfig::EnableSSL));
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_async_tcp_ssl_cfg_disable() {
        let mut client = make_test_client();
        let socket = client.allocate_tcp_sockets().unwrap();

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
            let (sock, _) = *client
                .callbacks
                .borrow_mut()
                .tcp_sockets
                .get(socket)
                .unwrap();

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
        let (sock, _) = *client
            .callbacks
            .borrow_mut()
            .tcp_sockets
            .get(socket)
            .unwrap();
        assert_eq!(
            sock.get_ssl_cfg(),
            u8::from(SslSockConfig::EnableSniValidation)
        );
    }

    #[test]
    fn test_async_get_winc_mac_address_success() {
        let mut client = make_test_client();
        let mac = client.get_winc_mac_address(true);

        assert!(mac.is_ok());
        assert_eq!(mac.unwrap().octets(), [0u8; 6]);
    }

    #[test]
    fn test_async_get_winc_mac_address_failure() {
        let mut client = make_test_client();
        let mac = client.get_winc_mac_address(false);

        assert_eq!(
            mac.err(),
            Some(StackError::WincWifiFail(Error::BufferReadError))
        );
    }

    #[test]
    fn test_async_get_firmware_version_ok() {
        let mut client = make_test_client();
        client.callbacks.borrow_mut().state = WifiModuleState::Unconnected;
        let result = client.get_firmware_version();
        assert_eq!(result.unwrap().chip_id, 0);
    }

    #[apply(test!)]
    async fn test_async_provisioning_mode_open_success() {
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

        let result = {
            // test client
            let mut client = make_test_client();
            *client.debug_callback.borrow_mut() = Some(&mut my_debug);
            // set the module state to unconnected.
            client.callbacks.borrow_mut().state = WifiModuleState::Unconnected;

            client
                .start_provisioning_mode(&ap, &hostname, false, 1)
                .await
        };

        assert!(result.is_ok());
        if let Ok(info) = result {
            assert_eq!(info.key, Credentials::Open);
            assert_eq!(info.ssid, test_ssid);
        } else {
            assert!(false);
        }
    }

    #[apply(test!)]
    async fn test_async_provisioning_mode_wpa_success() {
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

        let result = {
            // test client
            let mut client = make_test_client();
            *client.debug_callback.borrow_mut() = Some(&mut my_debug);
            // set the module state to unconnected.
            client.callbacks.borrow_mut().state = WifiModuleState::Unconnected;

            client
                .start_provisioning_mode(&ap, &hostname, false, 1)
                .await
        };

        assert!(result.is_ok());
        if let Ok(info) = result {
            assert_eq!(info.key, Credentials::WpaPSK(test_key));
            assert_eq!(info.ssid, test_ssid);
        } else {
            assert!(false);
        }
    }

    #[cfg(feature = "wep")]
    #[apply(test!)]
    async fn test_async_provisioning_mode_wep_success() {
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

        let result = {
            // test client
            let mut client = make_test_client();
            *client.debug_callback.borrow_mut() = Some(&mut my_debug);
            // set the module state to unconnected.
            client.callbacks.borrow_mut().state = WifiModuleState::Unconnected;
            client
                .start_provisioning_mode(&ap, &hostname, false, 1)
                .await
        };

        assert!(result.is_ok());
        if let Ok(info) = result {
            assert_eq!(info.key, Credentials::Wep(test_wep_key, wep_key_index));
            assert_eq!(info.ssid, test_ssid);
        } else {
            assert!(false);
        }
    }

    #[apply(test!)]
    async fn test_async_provisioning_mode_enterprise_fail() {
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

        let result = {
            // test client
            let mut client = make_test_client();
            // set the module state to unconnected.
            client.callbacks.borrow_mut().state = WifiModuleState::Unconnected;
            client
                .start_provisioning_mode(&ap, &hostname, false, 1)
                .await
        };

        assert!(result.is_err());
        if let Err(error) = result {
            assert_eq!(error, StackError::InvalidParameters);
        } else {
            assert!(false);
        }
    }

    #[apply(test!)]
    async fn test_async_provisioning_invalid_state() {
        // ssid for access point configuration.
        let ap_ssid = Ssid::from("ssid").unwrap();
        // access point configuration.
        let ap = AccessPoint::open(&ap_ssid);
        // hostname for access point.
        let hostname = HostName::from("admin").unwrap();

        let result = {
            // test client
            let mut client = make_test_client();
            // set the module state to connecting.
            client.callbacks.borrow_mut().state = WifiModuleState::ConnectingToAp;
            client
                .start_provisioning_mode(&ap, &hostname, false, 1)
                .await
        };

        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err, StackError::InvalidState);
        } else {
            assert!(false);
        }
    }

    #[apply(test!)]
    async fn test_async_provisioning_timeout() {
        // ssid for access point configuration.
        let ap_ssid = Ssid::from("ssid").unwrap();
        // access point configuration.
        let ap = AccessPoint::open(&ap_ssid);
        // hostname for access point.
        let hostname = HostName::from("admin").unwrap();

        let result = {
            // test client
            let mut client = make_test_client();
            // set the module state to unconnected.
            client.callbacks.borrow_mut().state = WifiModuleState::Unconnected;
            client
                .start_provisioning_mode(&ap, &hostname, false, 1500)
                .await
        };

        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err, StackError::GeneralTimeout);
        } else {
            assert!(false);
        }
    }

    #[apply(test!)]
    async fn test_async_provisioning_failed() {
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

        let result = {
            // test client
            let mut client = make_test_client();
            *client.debug_callback.borrow_mut() = Some(&mut my_debug);
            // set the module state to unconnected.
            client.callbacks.borrow_mut().state = WifiModuleState::Unconnected;
            client
                .start_provisioning_mode(&ap, &hostname, false, 1)
                .await
        };

        assert!(result.is_err());
        if let Err(error) = result {
            assert_eq!(error, StackError::WincWifiFail(Error::Failed));
        } else {
            assert!(false);
        }
    }
}
