use embedded_nal::nb;

use crate::manager::AuthType;
use crate::manager::ScanResult;

use super::StackError;
use super::WincClient;
use super::Xfer;

use crate::{debug, info};

#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(crate) enum WifiModuleState {
    Reset,
    Starting,
    Started,
    ConnectingToAp,
    ConnectedToAp,
    ConnectionFailed,
    Scanning,
    ScanDone,
    GettingScanResult,
    HaveScanResult,
}

impl<X: Xfer> WincClient<'_, X> {
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
            _ => Ok(()),
        }
    }

    pub fn scan(&mut self) -> nb::Result<u8, StackError> {
        match self.callbacks.state {
            WifiModuleState::Reset | WifiModuleState::Starting => {
                Err(nb::Error::Other(StackError::InvalidState))
            }
            WifiModuleState::Started => {
                self.dispatch_events()?;
                self.callbacks.state = WifiModuleState::Scanning;
                // This is ignored for active scan
                const PASSIVE_SCAN_TIME: u16 = 1000;
                self.manager
                    .send_scan(0xFF, PASSIVE_SCAN_TIME)
                    .map_err(|x| nb::Error::Other(StackError::WincWifiFail(x)))?;
                Err(nb::Error::WouldBlock)
            }
            WifiModuleState::Scanning => {
                self.dispatch_events()?;
                Err(nb::Error::WouldBlock)
            }
            WifiModuleState::ScanDone => {
                self.callbacks.state = WifiModuleState::Started;
                let num_aps = self.callbacks.connection_state.scan_number_aps.unwrap();
                debug!("Scan done, aps:{}", num_aps);
                Ok(num_aps)
            }
            _ => Ok(0),
        }
    }

    pub fn get_scan_result(&mut self, index: u8) -> nb::Result<ScanResult, StackError> {
        match self.callbacks.state {
            WifiModuleState::Started => {
                self.dispatch_events()?;
                self.callbacks.state = WifiModuleState::GettingScanResult;
                self.manager
                    .send_get_scan_result(index)
                    .map_err(|x| nb::Error::Other(StackError::WincWifiFail(x)))?;
                Err(nb::Error::WouldBlock)
            }
            WifiModuleState::GettingScanResult => {
                self.dispatch_events()?;
                Err(nb::Error::WouldBlock)
            }
            WifiModuleState::HaveScanResult => {
                self.callbacks.state = WifiModuleState::Started;
                let result = self.callbacks.connection_state.scan_results.take().unwrap();
                Ok(result)
            }
            _ => Err(nb::Error::Other(StackError::InvalidState)),
        }
    }
}
