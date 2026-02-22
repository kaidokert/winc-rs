use super::AsyncClient;
use super::StackError;
use crate::manager::{BootMode, BootState, Credentials, Ssid, WifiChannel};
use crate::net_ops::module::StationMode;
use crate::stack::socket_callbacks::WifiModuleState;
use crate::transfer::Xfer;

impl<X: Xfer> AsyncClient<'_, X> {
    /// Initializes the Wifi module in normal mode - boots the firmware and
    /// completes the remaining initialization.
    ///
    /// # Returns
    ///
    /// * `()` - The Wifi module has started successfully.
    /// * `StackError` - Starting the Wifi module failed.
    pub async fn start_wifi_module(&mut self) -> Result<(), StackError> {
        if self.callbacks.borrow().state != WifiModuleState::Reset {
            return Err(StackError::InvalidState);
        }
        self.callbacks.borrow_mut().state = WifiModuleState::Starting;
        self.manager.borrow_mut().set_crc_state(true);

        let mut state = BootState::new(BootMode::Normal);
        loop {
            let result = self.manager.borrow_mut().boot_the_chip(&mut state)?;
            if result {
                self.callbacks.borrow_mut().state = WifiModuleState::Unconnected;
                return Ok(());
            }
            self.dispatch_events()?;
            self.yield_once().await; // todo: busy loop, maybe should delay here
        }
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
    use crate::manager::{EventListener, WifiConnError, WifiConnState, WpaKey};
    use crate::stack::socket_callbacks::SocketCallbacks;
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
}
