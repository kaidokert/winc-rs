// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// As per the 'WiFi101' library, OTA notification APIs are not supported.
// - m2m_ota_notif_set_url
// - m2m_ota_notif_check_for_update
// - m2m_ota_notif_sched

use super::StackError;
use super::WincClient;
use super::Xfer;
use crate::manager::OtaRequest;
use crate::stack::socket_callbacks::{OtaUpdateError, OtaUpdateState, OtaUpdateStatus};
use crate::{error, info};
use embedded_nal::nb;

impl<X: Xfer> WincClient<'_, X> {
    /// Start the OTA update, either for the WINC1500 network stack or the Cortus processor.
    ///
    /// # Arguments
    ///
    /// * `server_url` - The server URL from which the firmware image will be downloaded.
    /// * `cortus_update` - `true`, the OTA update is for the Cortus else for network stack.
    ///
    /// **Note**: The last byte of the `server_url` array must be `0` (null-terminated).
    ///
    /// # Returns
    ///
    /// * `()` - If the OTA update was started successfully.
    /// * `StackError` - If an error occurs while starting the OTA update.
    pub fn start_ota_update(
        &mut self,
        server_url: &[u8],
        cortus_update: bool,
    ) -> nb::Result<(), StackError> {
        match &mut self.callbacks.ota_status {
            None => {
                self.manager
                    .send_start_ota_update(server_url, cortus_update)
                    .map_err(StackError::WincWifiFail)?;
                self.callbacks.ota_status = Some(None);
            }
            Some(ota_status) => {
                if let Some(status) = ota_status.take() {
                    self.callbacks.ota_status = None;
                    match status {
                        OtaUpdateStatus::Download(err) => {
                            if err == OtaUpdateError::NoError {
                                info!("OTA update image downloaded successfully");
                                return Ok(());
                            } else {
                                return Err(nb::Error::Other(StackError::OtaFail(err)));
                            }
                        }
                        _ => {
                            error!("OTA image download is interrupted: {:?}", status);
                            return Err(nb::Error::Other(StackError::OtaFail(status.into_error())));
                        }
                    }
                }
            }
        }
        self.dispatch_events()?;
        Err(nb::Error::WouldBlock)
    }

    /// Roll back the OTA update, either for the WINC1500 network stack or the Cortus processor.
    ///
    /// # Arguments
    ///
    /// * `cortus_update` - `true` to roll back the OTA update for the Cortus else for network stack.
    ///
    /// # Returns
    ///
    /// * `()` - Indicates that the OTA rollback request was successfully sent.
    /// * `StackError` - Returned if an error occurs while starting the OTA rollback.
    pub fn rollback_ota_update(&mut self, cortus_update: bool) -> nb::Result<(), StackError> {
        match &mut self.callbacks.ota.state {
            OtaUpdateState::Complete => {
                let request = if cortus_update {
                    OtaRequest::RollbackCortusFirmware
                } else {
                    OtaRequest::RollbackFirmware
                };
                self.manager
                    .send_ota_request(request)
                    .map_err(StackError::WincWifiFail)?;
                self.callbacks.ota.state = OtaUpdateState::RollingBack;
            }
            OtaUpdateState::RollingBack => {
                self.delay_us(self.poll_loop_delay_us); // absolute minimum delay to make timeout possible
                self.dispatch_events_may_wait()?;
                self.operation_countdown -= 1;
                if self.operation_countdown == 0 {
                    return Err(nb::Error::Other(StackError::GeneralTimeout));
                }
            }
            OtaUpdateState::Failed => {
                self.callbacks.ota.state = OtaUpdateState::NotStarted;
                error!("Unable to rollback update: {:?}", self.callbacks.ota.error);
                return Err(nb::Error::Other(StackError::OtaFail(
                    self.callbacks.ota.error,
                )));
            }
            OtaUpdateState::RolledBack => {
                info!(
                    "Image has been rolled back to an older version. Restart to make it effective."
                );
                return Ok(());
            }
            _ => return Err(nb::Error::Other(StackError::InvalidState)),
        }

        self.dispatch_events()?;
        Err(nb::Error::WouldBlock)
    }

    /// Abort The OTA update.
    ///
    /// # Returns
    ///
    /// * `()` - Indicates that the OTA update was aborted.
    /// * `StackError` - Returned if an error occurs while starting the OTA rollback.
    pub fn abort_ota(&mut self) -> Result<(), StackError> {
        self.manager
            .send_ota_request(OtaRequest::Abort)
            .map_err(StackError::WincWifiFail)?;
        Ok(())
    }

    /// Switches to the firmware updated via OTA.
    ///
    /// # Arguments
    ///
    /// * `switch_cortus_fw` - `true`, switch to the Cortus firmware else network stack firmware.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the firmware switch was successful.
    /// * `Err(StackError)` - If an error occurs while switching the firmware.
    pub fn switch_to_ota_firmware(&mut self, switch_cortus_fw: bool) -> Result<(), StackError> {
        let request = if switch_cortus_fw {
            OtaRequest::StartCortusFirmwareUpdate
        } else {
            OtaRequest::StartFirmwareUpdate
        };
        self.manager
            .send_ota_request(request)
            .map_err(StackError::WincWifiFail)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    //fn test_
}
