#[cfg(feature = "flash-rw")]
use crate::manager::BootMode;
use crate::manager::{BootState, Credentials, Manager, Ssid};
use crate::net_ops::op::OpImpl;
use crate::stack::socket_callbacks::{SocketCallbacks, WifiModuleState};
use crate::transfer::Xfer;
use crate::{StackError, WifiChannel};

// 1 minute max, if no other delays are added
const AP_CONNECT_TIMEOUT_MILLISECONDS: u32 = 60_000;

pub(crate) struct StationMode<'a> {
    ssid: Option<&'a Ssid>,
    credentials: Option<&'a Credentials>,
    channel: WifiChannel,
    save_credentials: bool,
    use_defaults: bool,
}

impl<'a> StationMode<'a> {
    /// Creates a new Wi-Fi configuration from the provided credentials.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID of the access point to connect to.
    /// * `credentials` - Authentication credentials for the access point.
    /// * `channel` - The Wi-Fi channel to use when connecting.
    /// * `save_credentials` - Whether the credentials should be stored
    ///   persistently after a successful connection.
    ///
    /// # Returns
    ///
    /// A `StationMode` instance configured with the provided SSID, credentials,
    /// channel, and credential persistence preference.
    pub(crate) fn from_credentials(
        ssid: &'a Ssid,
        credentials: &'a Credentials,
        channel: WifiChannel,
        save_credentials: bool,
    ) -> Self {
        Self {
            ssid: Some(ssid),
            credentials: Some(credentials),
            channel,
            save_credentials,
            use_defaults: false,
        }
    }

    /// Creates a new `StationMode` instance configured to use saved credentials.
    pub(crate) fn from_defaults() -> Self {
        Self {
            ssid: None,
            credentials: None,
            channel: WifiChannel::Channel1,
            save_credentials: false,
            use_defaults: true,
        }
    }
}

impl<X: Xfer> OpImpl<X> for StationMode<'_> {
    type Output = ();
    type Error = StackError;

    /// Polls the internal state machine and attempts to progress the connection.
    ///
    /// # Arguments
    ///
    /// * `manager` - The stack manager handling low-level operations.
    /// * `callbacks` - Socket callback handlers.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(output))` - Operation completed successfully.
    /// * `Ok(None)` - Operation is still in progress.
    /// * `Err(Self::Error)` - An error occurred while polling.
    fn poll_impl(
        &mut self,
        manager: &mut crate::manager::Manager<X>,
        callbacks: &mut crate::stack::socket_callbacks::SocketCallbacks,
    ) -> Result<Option<Self::Output>, Self::Error> {
        let state = callbacks.state.clone();

        match state {
            WifiModuleState::Unconnected => {
                callbacks.state = WifiModuleState::ConnectingToAp;
                manager.set_operation_timeout(AP_CONNECT_TIMEOUT_MILLISECONDS);
                if self.use_defaults {
                    manager.send_default_connect()?;
                } else {
                    manager.send_connect(
                        self.ssid.ok_or(StackError::InvalidParameters)?,
                        self.credentials.ok_or(StackError::InvalidParameters)?,
                        self.channel,
                        !self.save_credentials,
                    )?;
                }
            }
            WifiModuleState::ConnectionFailed => {
                callbacks.state = WifiModuleState::Unconnected;
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
                let mut timeout = manager.get_operation_timeout();
                if timeout == 0 {
                    return Err(StackError::GeneralTimeout);
                }
                timeout -= 1;
                manager.set_operation_timeout(timeout);
            }
            WifiModuleState::ConnectedToAp => {
                return Ok(Some(()));
            }
            _ => {
                return Err(StackError::InvalidState);
            }
        }

        Ok(None)
    }
}

impl<X: Xfer> OpImpl<X> for BootState {
    type Output = ();
    type Error = StackError;

    /// Polls the internal state machine and attempts to boot the chip.
    ///
    /// # Arguments
    ///
    /// * `manager` - The stack manager handling low-level operations.
    /// * `callbacks` - Socket callback handlers.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(output))` - Operation completed successfully.
    /// * `Ok(None)` - Operation is still in progress.
    /// * `Err(Self::Error)` - An error occurred while polling.
    fn poll_impl(
        &mut self,
        manager: &mut Manager<X>,
        callbacks: &mut SocketCallbacks,
    ) -> Result<Option<Self::Output>, Self::Error> {
        let state = callbacks.state.clone();
        match state {
            WifiModuleState::Reset => {
                manager.set_crc_state(true);
                callbacks.state = WifiModuleState::Starting;
            }
            WifiModuleState::Starting => {
                let result = manager.boot_the_chip(self)?;
                if result {
                    #[cfg(feature = "flash-rw")]
                    {
                        if self.get_boot_mode() == BootMode::Download {
                            callbacks.state = WifiModuleState::DownloadMode;
                            crate::info!("Chip booted into download mode.");
                            return Ok(Some(()));
                        }
                    }
                    callbacks.state = WifiModuleState::Unconnected;
                    return Ok(Some(()));
                }
            }
            #[cfg(feature = "flash-rw")]
            WifiModuleState::DownloadMode => {
                if self.get_boot_mode() == BootMode::Download {
                    crate::info!("Chip is already in download mode.");
                    return Ok(Some(()));
                }
                return Err(StackError::InvalidState);
            }
            _ => return Err(StackError::InvalidState),
        }

        Ok(None)
    }
}
