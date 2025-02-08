use embedded_nal::nb;

use crate::manager::AuthType;

use super::StackError;
use super::WincClient;
use super::Xfer;

use crate::{debug, info};

#[derive(Debug, PartialEq)]
pub(crate) enum WifiModuleState {
    Reset,
    Starting,
    Started,
    ConnectingToAp,
    ConnectedToAp,
    ConnectionFailed,
    HaveIp,
    SystemTimeReceived,
}

impl<'a, X: Xfer> WincClient<'a, X> {
    pub fn heartbeat(&mut self) -> Result<(), StackError> {
        self.dispatch_events()?;
        Ok(())
    }

    // TODO: refactor this to use nb::Result, no callback
    pub fn start_module(
        &mut self,
        wait_callback: &mut dyn FnMut(u32) -> bool,
    ) -> Result<(), StackError> {
        if self.callbacks.state != WifiModuleState::Reset {
            return Err(StackError::InvalidState);
        }
        self.callbacks.state = WifiModuleState::Starting;
        self.manager.set_crc_state(true);
        self.manager.start(wait_callback)?;
        self.callbacks.state = WifiModuleState::Started;

        Ok(())
    }
    pub fn connect_to_ap(&mut self, ssid: &str, password: &str) -> nb::Result<(), StackError> {
        match self.callbacks.state {
            WifiModuleState::Reset | WifiModuleState::Starting => {
                Err(nb::Error::Other(StackError::InvalidState))
            }
            WifiModuleState::Started => {
                self.callbacks.state = WifiModuleState::ConnectingToAp;
                self.manager
                    .send_connect(AuthType::WpaPSK, ssid, password, 0xFF, false)
                    .map_err(|x| nb::Error::Other(StackError::WincWifiFail(x)))?;
                Err(nb::Error::WouldBlock)
            }
            WifiModuleState::ConnectingToAp => {
                self.dispatch_events()?;
                Err(nb::Error::WouldBlock)
            }
            WifiModuleState::ConnectionFailed => Err(nb::Error::Other(StackError::ApJoinFailed(
                self.callbacks.connection_state.conn_error.take().unwrap(),
            ))),
            WifiModuleState::ConnectedToAp => {
                info!("connect_to_ap: got Connected to AP");
                Ok(())
            }

            WifiModuleState::HaveIp => {
                info!("connect_to_ap: got Have IP");
                Ok(())
            }
            WifiModuleState::SystemTimeReceived => {
                info!("connect_to_ap: got time received");
                Ok(())
            }
        }
    }
}
