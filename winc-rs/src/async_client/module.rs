use super::AsyncClient;
use super::StackError;
use crate::manager::{BootMode, BootState, Credentials, Ssid, WifiChannel};
use crate::net_ops::module::StationMode;
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
}

#[cfg(test)]
mod tests {
    use super::super::tests::make_test_client;
    use super::*;
    use crate::errors::CommError as Error;
    use crate::manager::{EventListener, WifiConnError, WifiConnState, WpaKey};
    use crate::stack::socket_callbacks::{SocketCallbacks, WifiModuleState};
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
}
