use super::AsyncClient;
use super::StackError;
use crate::manager::{
    AccessPoint, BootMode, BootState, Credentials, HostName, ProvisioningInfo, Ssid, WifiChannel,
};
use crate::net_ops::module::{ProvisioningMode, StationMode};
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
    use crate::stack::socket_callbacks::{SocketCallbacks, WifiModuleState};
    use core::net::Ipv4Addr;
    use macro_rules_attribute::apply;
    use smol_macros::test;

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
