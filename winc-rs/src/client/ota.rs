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
use crate::stack::socket_callbacks::OtaUpdateState;
use crate::{error, info};
use embedded_nal::nb;

/// Default timeout for OTA request is 1 minute.
const OTA_REQUEST_TIMEOUT: u32 = 60_000;
/// Default timeout to update the firmware is 5 minutes.
const OTA_UPDATE_TIMEOUT: u32 = 5 * 60_000;

impl<X: Xfer> WincClient<'_, X> {
    /// Start the downloading the OTA update, either for the WINC1500 network stack or the Cortus processor.
    ///
    /// # Arguments
    ///
    /// * `server_url` - The server URL from which the firmware image will be downloaded.
    /// * `cortus_update` - If `true`, the OTA update is for the Cortus processor else for network stack.
    /// * `timeout` - Optional timeout duration in milliseconds for the operation. Defaults to 5 minutes.
    ///
    /// **Notes**:
    /// * The last byte of the `server_url` array must be `0` (null-terminated).
    /// * If the OTA download fails due to a general timeout, it is recommended to send an abort request to
    /// cancel the previous update request.
    ///
    /// # Returns
    ///
    /// * `()` - If the OTA update was downloaded successfully.
    /// * `StackError` - If an error occurs while downloading the OTA update.
    pub fn start_ota_update(
        &mut self,
        server_url: &[u8],
        cortus_update: bool,
        timeout: Option<u32>,
    ) -> nb::Result<(), StackError> {
        // URL should be non-empty and null terminated.
        if server_url.is_empty() || !server_url.ends_with(&[0]) {
            return Err(nb::Error::Other(StackError::InvalidParameters));
        }

        match self.callbacks.ota_state {
            OtaUpdateState::NotStarted => {
                self.manager
                    .send_start_ota_update(server_url, cortus_update)?;
                self.callbacks.ota_state = OtaUpdateState::InProgress;
                if let Some(time) = timeout {
                    self.operation_countdown = time;
                } else {
                    self.operation_countdown = OTA_UPDATE_TIMEOUT;
                }
            }
            OtaUpdateState::InProgress => {
                self.delay_us(self.poll_loop_delay_us);
                self.operation_countdown -= 1;
                if self.operation_countdown == 0 {
                    error!("OTA update timed out. Consider sending an abort request before retrying the update.");
                    return Err(nb::Error::Other(StackError::GeneralTimeout));
                }
            }
            OtaUpdateState::Complete => {
                info!("OTA update image downloaded.");
                return Ok(());
            }
            OtaUpdateState::Failed(e) => {
                self.callbacks.ota_state = OtaUpdateState::NotStarted;
                error!("Unable to download the OTA update: {:?}", e);
                return Err(nb::Error::Other(StackError::OtaFail(e)));
            }
            _ => return Err(nb::Error::Other(StackError::InvalidState)),
        }

        self.dispatch_events_may_wait()?;
        Err(nb::Error::WouldBlock)
    }

    /// Roll back the OTA update, either for the WINC1500 network stack or the Cortus processor.
    ///
    /// # Arguments
    ///
    /// * `cortus_update` - If `true`, roll back the OTA update for the Cortus processor else for network stack.
    ///
    /// # Returns
    ///
    /// * `()` - Indicates that the OTA rollback request was successfully sent.
    /// * `StackError` - Returned if an error occurs while starting the OTA rollback.
    pub fn rollback_ota_update(
        &mut self,
        cortus_update: bool,
        timeout: Option<u32>,
    ) -> nb::Result<(), StackError> {
        match self.callbacks.ota_state {
            OtaUpdateState::Complete | OtaUpdateState::NotStarted => {
                let request = if cortus_update {
                    OtaRequest::RollbackCortusFirmware
                } else {
                    OtaRequest::RollbackFirmware
                };
                self.manager.send_ota_request(request)?;
                self.callbacks.ota_state = OtaUpdateState::RollingBack;
                if let Some(time) = timeout {
                    self.operation_countdown = time;
                } else {
                    self.operation_countdown = OTA_REQUEST_TIMEOUT;
                }
            }
            OtaUpdateState::RollingBack => {
                self.delay_us(self.poll_loop_delay_us);
                self.operation_countdown -= 1;
                if self.operation_countdown == 0 {
                    self.callbacks.ota_state = OtaUpdateState::NotStarted;
                    error!("Rolling back the OTA update timed out.");
                    return Err(nb::Error::Other(StackError::GeneralTimeout));
                }
            }
            OtaUpdateState::Failed(e) => {
                self.callbacks.ota_state = OtaUpdateState::NotStarted;
                error!("Unable to rollback update: {:?}", e);
                return Err(nb::Error::Other(StackError::OtaFail(e)));
            }
            OtaUpdateState::RolledBack => {
                info!("Image has been rolled back, restart to make it effective.");
                return Ok(());
            }
            _ => return Err(nb::Error::Other(StackError::InvalidState)),
        }

        self.dispatch_events_may_wait()?;
        Err(nb::Error::WouldBlock)
    }

    /// Abort The OTA update.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Optional timeout duration in milliseconds for the operation. Defaults to 1 minutes.
    ///
    /// # Returns
    ///
    /// * `()` - OTA update was aborted.
    /// * `StackError` - Returned if an error occurs while starting the OTA rollback.
    pub fn abort_ota(&mut self, timeout: Option<u32>) -> nb::Result<(), StackError> {
        match self.callbacks.ota_state {
            OtaUpdateState::NotStarted | OtaUpdateState::InProgress => {
                self.manager.send_ota_request(OtaRequest::Abort)?;
                self.callbacks.ota_state = OtaUpdateState::Aborting;
                if let Some(time) = timeout {
                    self.operation_countdown = time;
                } else {
                    self.operation_countdown = OTA_REQUEST_TIMEOUT;
                }
            }
            OtaUpdateState::Aborting => {
                self.delay_us(self.poll_loop_delay_us);
                self.operation_countdown -= 1;
                if self.operation_countdown == 0 {
                    self.callbacks.ota_state = OtaUpdateState::NotStarted;
                    error!("OTA update abort timed out.");
                    return Err(nb::Error::Other(StackError::GeneralTimeout));
                }
            }
            OtaUpdateState::Aborted => {
                info!("OTA update was aborted successfully.");
                return Ok(());
            }
            OtaUpdateState::Failed(e) => {
                self.callbacks.ota_state = OtaUpdateState::NotStarted;
                error!("Unable to abort the update: {:?}", e);
                return Err(nb::Error::Other(StackError::OtaFail(e)));
            }
            _ => return Err(nb::Error::Other(StackError::InvalidState)),
        }

        self.dispatch_events_may_wait()?;
        Err(nb::Error::WouldBlock)
    }

    /// Switches to the firmware updated via OTA.
    ///
    /// # Arguments
    ///
    /// * `switch_cortus_fw` - `true`, switch to the Cortus firmware else network stack firmware.
    /// * `timeout` - Optional timeout duration in milliseconds for the operation. Defaults to 1 minutes.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the firmware switch was successful.
    /// * `Err(StackError)` - If an error occurs while switching the firmware.
    pub fn switch_to_ota_firmware(
        &mut self,
        switch_cortus_fw: bool,
        timeout: Option<u32>,
    ) -> nb::Result<(), StackError> {
        match self.callbacks.ota_state {
            OtaUpdateState::Complete | OtaUpdateState::NotStarted => {
                let request = if switch_cortus_fw {
                    OtaRequest::SwitchCortusFirmware
                } else {
                    OtaRequest::SwitchFirmware
                };
                self.manager.send_ota_request(request)?;
                self.callbacks.ota_state = OtaUpdateState::SwitchingFirmware;
                if let Some(time) = timeout {
                    self.operation_countdown = time;
                } else {
                    self.operation_countdown = OTA_REQUEST_TIMEOUT;
                }
            }
            OtaUpdateState::SwitchingFirmware => {
                self.delay_us(self.poll_loop_delay_us);
                self.operation_countdown -= 1;
                if self.operation_countdown == 0 {
                    self.callbacks.ota_state = OtaUpdateState::NotStarted;
                    error!("Switching to the updated OTA firmware timed out.");
                    return Err(nb::Error::Other(StackError::GeneralTimeout));
                }
            }
            OtaUpdateState::Switched => {
                info!("OTA firmware was switched successfully. Restart to make it effective.");
                return Ok(());
            }
            OtaUpdateState::Failed(e) => {
                self.callbacks.ota_state = OtaUpdateState::NotStarted;
                error!("Unable to switch to updated firmware: {:?}", e);
                return Err(nb::Error::Other(StackError::OtaFail(e)));
            }
            _ => return Err(nb::Error::Other(StackError::InvalidState)),
        }

        self.dispatch_events_may_wait()?;
        Err(nb::Error::WouldBlock)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::client::{test_shared::*, SocketCallbacks};
    use crate::manager::{EventListener, OtaUpdateError, OtaUpdateStatus};

    #[test]
    fn test_ota_update_success() {
        let mut client = make_test_client();
        let server = b"www.google.com\0";
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_ota(OtaUpdateStatus::Download, OtaUpdateError::NoError);
        };

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.start_ota_update(server, false, None));

        assert!(result.is_ok());
    }

    #[test]
    fn test_ota_cortus_update_success() {
        let mut client = make_test_client();
        let server = b"www.google.com\0";
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_ota(OtaUpdateStatus::Download, OtaUpdateError::NoError);
        };

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.start_ota_update(server, true, None));

        assert!(result.is_ok());
    }

    #[test]
    fn test_ota_update_failure() {
        let mut client = make_test_client();
        let server = b"www.google.com\0";
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_ota(OtaUpdateStatus::Download, OtaUpdateError::ServerError);
        };

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.start_ota_update(server, true, None));

        assert_eq!(
            result.err(),
            Some(StackError::OtaFail(OtaUpdateError::ServerError))
        );
    }

    #[test]
    fn test_ota_update_invalid_state() {
        let mut client = make_test_client();
        let server = b"www.google.com\0";

        client.callbacks.ota_state = OtaUpdateState::Aborted;

        let result = nb::block!(client.start_ota_update(server, true, None));

        assert_eq!(result.err(), Some(StackError::InvalidState));
    }

    #[test]
    fn test_ota_update_timeout() {
        let mut client = make_test_client();
        let server = b"www.google.com\0";

        let result = nb::block!(client.start_ota_update(server, true, Some(100)));

        assert_eq!(result.err(), Some(StackError::GeneralTimeout));
    }

    #[test]
    fn test_ota_update_inprogress() {
        let mut client = make_test_client();
        let server = b"www.google.com\0";
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_ota(OtaUpdateStatus::Download, OtaUpdateError::UpdateInProgress);
        };

        client.debug_callback = Some(&mut my_debug);

        let _ = client.start_ota_update(server, true, None);

        let result = client.start_ota_update(server, true, None);

        assert_eq!(result.err(), Some(nb::Error::WouldBlock));
    }

    #[test]
    fn test_ota_update_invalid_url() {
        let mut client = make_test_client();
        let server = b"www.google.com";

        let result = nb::block!(client.start_ota_update(server, true, None));

        assert_eq!(result.err(), Some(StackError::InvalidParameters));
    }

    #[test]
    fn test_ota_update_empty_url() {
        let mut client = make_test_client();
        let server = b"";

        let result = nb::block!(client.start_ota_update(server, true, None));

        assert_eq!(result.err(), Some(StackError::InvalidParameters));
    }

    #[test]
    fn test_abort_ota_update_success() {
        let mut client = make_test_client();
        let server = b"www.google.com\0";

        let result = client.start_ota_update(server, true, None);

        assert_eq!(result.err(), Some(nb::Error::WouldBlock));

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_ota(OtaUpdateStatus::Abort, OtaUpdateError::NoError);
        };

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.abort_ota(None));

        assert!(result.is_ok());
    }

    #[test]
    fn test_abort_ota_update_failure() {
        let mut client = make_test_client();
        let server = b"www.google.com\0";

        let result = client.start_ota_update(server, true, None);

        assert_eq!(result.err(), Some(nb::Error::WouldBlock));

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_ota(OtaUpdateStatus::Abort, OtaUpdateError::GenericFail);
        };

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.abort_ota(None));

        assert_eq!(
            result.err(),
            Some(StackError::OtaFail(OtaUpdateError::GenericFail))
        );
    }

    #[test]
    fn test_abort_ota_update_timeout() {
        let mut client = make_test_client();
        client.callbacks.ota_state = OtaUpdateState::InProgress;

        let result = nb::block!(client.abort_ota(Some(100)));

        assert_eq!(result.err(), Some(StackError::GeneralTimeout));
    }

    #[test]
    fn test_abort_ota_update_invalid_state() {
        let mut client = make_test_client();
        client.callbacks.ota_state = OtaUpdateState::Complete;

        let result = nb::block!(client.abort_ota(None));

        assert_eq!(result.err(), Some(StackError::InvalidState));
    }

    #[test]
    fn test_ota_switch_fw_success() {
        let mut client = make_test_client();
        client.callbacks.ota_state = OtaUpdateState::Complete;

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_ota(OtaUpdateStatus::SwitchingFirmware, OtaUpdateError::NoError);
        };

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.switch_to_ota_firmware(true, None));

        assert!(result.is_ok());
    }

    #[test]
    fn test_ota_switch_fw_invalid_state() {
        let mut client = make_test_client();
        let server = b"www.google.com\0";

        let result = client.start_ota_update(server, true, None);

        assert_eq!(result.err(), Some(nb::Error::WouldBlock));

        let result = client.switch_to_ota_firmware(false, None);

        assert_eq!(
            result.err(),
            Some(nb::Error::Other(StackError::InvalidState))
        );
    }

    #[test]
    fn test_ota_switch_fw_faliure() {
        let mut client = make_test_client();
        client.callbacks.ota_state = OtaUpdateState::NotStarted;

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_ota(
                OtaUpdateStatus::SwitchingFirmware,
                OtaUpdateError::ImageVerificationFailed,
            );
        };

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.switch_to_ota_firmware(true, None));

        assert_eq!(
            result.err(),
            Some(StackError::OtaFail(OtaUpdateError::ImageVerificationFailed))
        );
    }

    #[test]
    fn test_ota_switch_fw_timeout() {
        let mut client = make_test_client();
        client.callbacks.ota_state = OtaUpdateState::NotStarted;

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_ota(
                OtaUpdateStatus::Download,
                OtaUpdateError::ImageVerificationFailed,
            );
        };

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.switch_to_ota_firmware(false, Some(100)));

        assert_eq!(result.err(), Some(StackError::GeneralTimeout));
    }

    #[test]
    fn test_ota_rollback_fw_success() {
        let mut client = make_test_client();
        client.callbacks.ota_state = OtaUpdateState::NotStarted;

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_ota(OtaUpdateStatus::Rollback, OtaUpdateError::NoError);
        };

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.rollback_ota_update(true, None));

        assert!(result.is_ok());
    }

    #[test]
    fn test_ota_rollback_fw_failure() {
        let mut client = make_test_client();
        client.callbacks.ota_state = OtaUpdateState::NotStarted;

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_ota(
                OtaUpdateStatus::Rollback,
                OtaUpdateError::InvalidRollbackImage,
            );
        };

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.rollback_ota_update(false, None));

        assert_eq!(
            result.err(),
            Some(StackError::OtaFail(OtaUpdateError::InvalidRollbackImage))
        );
    }

    #[test]
    fn test_ota_rollback_fw_timeout() {
        let mut client = make_test_client();
        client.callbacks.ota_state = OtaUpdateState::NotStarted;

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_ota(OtaUpdateStatus::Abort, OtaUpdateError::InvalidRollbackImage);
        };

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.rollback_ota_update(true, Some(100)));

        assert_eq!(result.err(), Some(StackError::GeneralTimeout));
    }

    #[test]
    fn test_ota_rollback_fw_invalid_state() {
        let mut client = make_test_client();
        client.callbacks.ota_state = OtaUpdateState::InProgress;

        let result = nb::block!(client.rollback_ota_update(true, Some(100)));

        assert_eq!(result.err(), Some(StackError::InvalidState));
    }
}
