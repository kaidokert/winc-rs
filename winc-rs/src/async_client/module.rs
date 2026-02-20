use super::AsyncClient;
use super::StackError;
use crate::manager::{BootMode, BootState, Credentials, Ssid, WifiChannel};
use crate::stack::socket_callbacks::WifiModuleState;
use crate::transfer::Xfer;

// todo: deduplicate this
// 1 minute max, if no other delays are added
const AP_CONNECT_TIMEOUT_MILLISECONDS: u32 = 60_000;

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

    /// Connects with the access point by calling the provided connection function.
    ///
    /// # Arguments
    ///
    /// * `connect_fn` - A closure taking `&mut self` that performs the low-level
    ///   connection operation and returns a `CommError` on failure.
    ///
    /// # Returns
    ///
    /// * `()` - Successfully connected to the access point.
    /// * `StackError` - If an error occurs while connecting.
    async fn connect_to_ap_impl(
        &mut self,
        connect_fn: impl Fn(&mut Self) -> Result<(), crate::errors::CommError>,
    ) -> Result<(), StackError> {
        let mut countdown = AP_CONNECT_TIMEOUT_MILLISECONDS;

        loop {
            let read_state = self.callbacks.borrow().state.clone();
            match read_state {
                WifiModuleState::Unconnected => {
                    self.callbacks.borrow_mut().state = WifiModuleState::ConnectingToAp;
                    connect_fn(self)?;
                }
                WifiModuleState::ConnectionFailed => {
                    let mut callbacks = self.callbacks.borrow_mut();
                    // conn_error should always be Some in ConnectionFailed state,
                    // but use defensive fallback just in case
                    let res = callbacks
                        .connection_state
                        .conn_error
                        .take()
                        .unwrap_or(crate::manager::WifiConnError::Unhandled);
                    return Err(StackError::ApJoinFailed(res));
                }
                WifiModuleState::ConnectingToAp => {
                    countdown -= 1;
                    if countdown == 0 {
                        return Err(StackError::GeneralTimeout);
                    }
                }
                WifiModuleState::ConnectedToAp => {
                    return Ok(());
                }
                _ => {
                    return Err(StackError::InvalidState);
                }
            }
            self.dispatch_events()?;
            self.yield_once().await;
        }
    }

    /// Connect to access point with previously saved credentials.
    pub async fn connect_to_saved_ap(&mut self) -> Result<(), StackError> {
        self.connect_to_ap_impl(|inner_self: &mut Self| {
            inner_self.manager.borrow_mut().send_default_connect()
        })
        .await
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
        self.connect_to_ap_impl(|inner_self: &mut Self| {
            inner_self.manager.borrow_mut().send_connect(
                ssid,
                credentials,
                channel,
                !save_credentials,
            )
        })
        .await
    }
}
