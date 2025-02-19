use embedded_nal::nb;

use crate::manager::{AuthType, FirmwareInfo, ScanResult};

use super::PingResult;
use super::StackError;
use super::WincClient;
use super::Xfer;

use crate::info;

#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(crate) enum WifiModuleState {
    Reset,
    Starting,
    Started,
    ConnectingToAp,
    ConnectedToAp,
    ConnectionFailed,
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

    fn connect_to_ap_impl(
        &mut self,
        connect_fn: impl FnOnce(&mut Self) -> Result<(), crate::errors::Error>,
    ) -> nb::Result<(), StackError> {
        match self.callbacks.state {
            WifiModuleState::Reset | WifiModuleState::Starting => {
                Err(nb::Error::Other(StackError::InvalidState))
            }
            WifiModuleState::Started => {
                self.callbacks.state = WifiModuleState::ConnectingToAp;
                connect_fn(self).map_err(|x| nb::Error::Other(StackError::WincWifiFail(x)))?;
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
        }
    }

    pub fn connect_to_saved_ap(&mut self) -> nb::Result<(), StackError> {
        self.connect_to_ap_impl(|inner_self: &mut Self| inner_self.manager.send_default_connect())
    }

    pub fn connect_to_ap(&mut self, ssid: &str, password: &str) -> nb::Result<(), StackError> {
        self.connect_to_ap_impl(|inner_self: &mut Self| {
            inner_self
                .manager
                .send_connect(AuthType::WpaPSK, ssid, password, 0xFF, false)
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
                self.manager
                    .send_scan(0xFF, PASSIVE_SCAN_TIME)
                    .map_err(|x| nb::Error::Other(StackError::WincWifiFail(x)))?;
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

        self.dispatch_events()?;
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
                self.manager
                    .send_get_scan_result(index)
                    .map_err(StackError::WincWifiFail)?;
                self.callbacks.connection_state.scan_results = Some(None);
            }
            Some(result) => {
                if let Some(result) = result.take() {
                    self.callbacks.connection_state.scan_results = None;
                    return Ok(result);
                }
            }
        }

        self.dispatch_events()?;
        Err(nb::Error::WouldBlock)
    }

    /// Gets current RSSI level
    pub fn get_current_rssi(&mut self) -> nb::Result<i8, StackError> {
        match &mut self.callbacks.connection_state.rssi_level {
            None => {
                self.manager
                    .send_get_current_rssi()
                    .map_err(StackError::WincWifiFail)?;
                self.callbacks.connection_state.rssi_level = Some(None);
            }
            Some(rssi) => {
                if let Some(rssi) = rssi.take() {
                    self.callbacks.connection_state.rssi_level = None;
                    return Ok(rssi);
                }
            }
        }

        self.dispatch_events()?;
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
                self.manager
                    .send_get_conn_info()
                    .map_err(StackError::WincWifiFail)?;
                self.callbacks.connection_state.conn_info = Some(None);
            }
            Some(info) => {
                if let Some(info) = info.take() {
                    self.callbacks.connection_state.conn_info = None;
                    return Ok(info);
                }
            }
        }
        self.dispatch_events()?;
        Err(nb::Error::WouldBlock)
    }

    /// Get the firmware version of the Wifi module
    pub fn get_firmware_version(&mut self) -> Result<FirmwareInfo, StackError> {
        self.manager
            .get_firmware_ver_full()
            .map_err(StackError::WincWifiFail)
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
                self.manager
                    .send_ping_req(dest_ip, ttl, count, marker)
                    .map_err(StackError::WincWifiFail)?;
                self.callbacks.connection_state.ping_result = Some(None);
            }
            Some(result) => {
                if let Some(result) = result.take() {
                    self.callbacks.connection_state.ping_result = None;
                    return Ok(result);
                }
            }
        }

        self.dispatch_events()?;
        Err(nb::Error::WouldBlock)
    }
}
