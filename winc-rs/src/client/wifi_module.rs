use embedded_nal::nb;

use crate::manager::{AuthType, FirmwareInfo, ScanResult};

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

    /// Get the firmware version of the Wifi module
    pub fn get_firmware_version(&mut self) -> Result<FirmwareInfo, StackError> {
        self.manager
            .get_firmware_ver_full()
            .map_err(StackError::WincWifiFail)
    }

    /// TODO: Not implemented yet: send a ping request
    pub fn send_ping(
        &mut self,
        dest_ip: core::net::Ipv4Addr,
        ttl: u8,
        count: u16,
        marker: u8,
    ) -> Result<(), StackError> {
        self.manager
            .send_ping_req(dest_ip, ttl, count, marker)
            .map_err(StackError::WincWifiFail)?;
        todo!()
    }

    /// TODO: Not implemented yet: get the current RSSI
    pub fn get_current_rssi(&mut self) -> Result<(), StackError> {
        // Send is done, need to plumb response back
        self.manager
            .send_get_current_rssi()
            .map_err(StackError::WincWifiFail)?;
        Ok(())
    }

    /// TODO: Not implemented yet: get the connection info
    pub fn get_connection_info(&mut self) -> Result<(), StackError> {
        // Send is done, need to plumb response back
        self.manager
            .send_get_conn_info()
            .map_err(StackError::WincWifiFail)?;
        Ok(())
    }
}
