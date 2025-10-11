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

use embedded_nal::nb;

use super::{StackError, WincClient, Xfer};
#[cfg(feature = "experimental-ecc")]
use crate::error;

#[cfg(feature = "experimental-ecc")]
use crate::manager::{EccInfo, EccPoint, EccRequestType, EcdhInfo};

use crate::manager::{SslCertExpiryOpt, SslCipherSuite};

// Default timeout to wait for response of SSL request is 1 second.
const SSL_REQ_TIMEOUT: u32 = 1000; 

impl<X: Xfer> WincClient<'_, X> {
    /// Configure the SSL certificate expiry option.
    ///
    /// # Arguments
    ///
    /// * `opt` – The SSL certificate expiry option to apply.
    ///
    /// # Returns
    ///
    /// * `Ok(())` – If the request was successfully processed.
    /// * `Err(StackError)` – If an error occurred while configuring the option.
    pub fn ssl_check_cert_expiry(&mut self, opt: SslCertExpiryOpt) -> Result<(), StackError> {
        Ok(self.manager.send_ssl_cert_expiry(opt)?)
    }

    /// Sets the SSL/TLS cipher suite for the WINC module.
    ///
    /// # Arguments
    ///
    /// * `ssl_cipher` - The cipher suite to be set.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the cipher suite was successfully set.
    /// * `Err(StackError)` - If an error occurred while configuring the cipher suite.
    pub fn ssl_set_cipher_suite(&mut self, ssl_cipher: SslCipherSuite) -> nb::Result<(), StackError> {
        match self.callbacks.ssl_cb_info.cipher_suite_bitmap {
            None => {
                self.manager.send_ssl_set_cipher_suite(ssl_cipher.into())?;
                self.operation_countdown = SSL_REQ_TIMEOUT;
                self.callbacks.ssl_cb_info.cipher_suite_bitmap = Some(None);
            }
            Some(rcvd_cs_opt) => {
                if let Some(rcvd_cs_bitmap) = rcvd_cs_opt {
                    self.callbacks.ssl_cb_info.cipher_suite_bitmap = None;
                    if rcvd_cs_bitmap == u32::from(ssl_cipher) {
                        return Ok(());
                    } else {
                        return Err(nb::Error::Other(StackError::InvalidResponse));
                    }
                } else {
                    self.delay_us(self.poll_loop_delay_us);
                    self.operation_countdown -= 1;
                    if self.operation_countdown == 0 {
                        self.callbacks.ssl_cb_info.cipher_suite_bitmap = None;
                        return Err(nb::Error::Other(StackError::GeneralTimeout));
                    }
                }
            }
        }

        self.dispatch_events_may_wait()?;
        Err(nb::Error::WouldBlock)
    }

    /// Sends an ECC handshake response to the module.
    ///
    /// An ECC handshake request is received from the WINC, and
    /// a response is sent back to the WINC.
    ///
    /// # Arguments
    ///
    /// * `ecc_info` – A reference to the ECC operation information structure.
    /// * `ecdh_info` – A reference to the ECDH information structure.
    /// * `resp_buffer` – A buffer containing the ECC response.
    ///
    /// # Returns
    ///
    /// * `Ok(())` – If the response was successfully sent.
    /// * `Err(StackError)` – If an error occurred while sending the response.
    #[cfg(feature = "experimental-ecc")]
    pub fn ssl_send_ecc_resp(
        &mut self,
        ecc_info: &EccInfo,
        ecdh_info: &EcdhInfo,
        resp_buffer: &[u8],
    ) -> Result<(), StackError> {
        // clear the previously acquired ECC HIF register.
        if let Some(ecc_req) = self.callbacks.ssl_cb_info.ecc_req.as_mut() {
            ecc_req.hif_reg = 0;
        }

        Ok(self
            .manager
            .send_ecc_resp(ecc_info, ecdh_info, resp_buffer)?)
    }

    /// Reads the SSL certificate from the WINC module.
    ///
    /// This function only attempts to read the certificate if an ECC request of type
    /// `EccRequestType::VerifySignature` is received from the WINC module.
    ///
    /// # Arguments
    ///
    /// * `curve_type` – A mutable reference to store the ECC curve type.
    /// * `hash` – A mutable buffer to store the hash value.
    /// * `signature` – A mutable buffer to store the ECC signature.
    /// * `ecc_point` – A mutable reference to store the ECC public key point.
    ///
    /// # Returns
    ///
    /// * `Ok(())` – If the certificate was successfully read.
    /// * `Err(StackError)` – If an error occurred while reading the certificate.
    #[cfg(feature = "experimental-ecc")]
    pub fn ssl_read_certificate(
        &mut self,
        curve_type: &mut u16,
        hash: &mut [u8],
        signature: &mut [u8],
        ecc_point: &mut EccPoint,
    ) -> Result<(), StackError> {
        match self.callbacks.ssl_cb_info.ecc_req.as_ref() {
            None => {
                error!("ECC request is not received from the module.");
                return Err(StackError::InvalidState);
            }
            Some(ecc_req) => {
                // Check if the ECC request type is valid.
                if ecc_req.ecc_info.req != EccRequestType::VerifySignature {
                    error!(
                        "Received ECC request type is invalid for this operation. Expected: {:?}, got: {:?}.",
                        EccRequestType::VerifySignature,
                        ecc_req.ecc_info.req
                    );
                    return Err(StackError::InvalidState);
                }

                let mut hif_addr = self
                    .callbacks
                    .ssl_cb_info
                    .ecc_req
                    .as_ref()
                    .map(|ecc_req| ecc_req.hif_reg)
                    .ok_or(StackError::InvalidState)?;

                let mut opts = [0u8; 8]; // read the ssl options.

                // Read the Curve Type, Key, Hash and Signature size.
                self.manager.read_ecc_info(hif_addr, &mut opts)?;
                hif_addr += 8;

                // Parse the values from the buffer
                *curve_type = u16::from_be_bytes([opts[0], opts[1]]);
                ecc_point.point_size = u16::from_be_bytes([opts[2], opts[3]]);
                let hash_size = u16::from_be_bytes([opts[4], opts[5]]);
                let sig_size = u16::from_be_bytes([opts[6], opts[7]]);

                // Read the ECC Point-X
                let to_read_len = (ecc_point.point_size * 2) as usize;
                if ecc_point.x_cord.len() < to_read_len {
                    return Err(StackError::InvalidParameters);
                }

                self.manager
                    .read_ecc_info(hif_addr, &mut ecc_point.x_cord[..to_read_len])?;
                hif_addr += to_read_len as u32;

                // Read the hash
                self.manager
                    .read_ecc_info(hif_addr, &mut hash[..hash_size as usize])?;
                hif_addr += hash_size as u32;

                // Read the Signature
                self.manager
                    .read_ecc_info(hif_addr, &mut signature[..sig_size as usize])?;

                Ok(())
            }
        }
    }

    /// Clears the ECC information available to read from the WINC module.
    ///
    /// This function should only be called if an ECC request of type
    /// `EccRequestType::VerifySignature` or `EccRequestType::GenerateSignature`
    /// has been received from the WINC module.
    /// It must not be called if all information has already been read,
    /// as calling it in that case will clear all remaining information.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the ECC information was successfully cleared.
    /// * `Err(StackError)` - If an error occurred while clearing the information.
    #[cfg(feature = "experimental-ecc")]
    pub fn ssl_clear_ecc_readable(&mut self) -> Result<(), StackError> {
        // check if ecc request is received from the module.
        match self.callbacks.ssl_cb_info.ecc_req.as_ref() {
            Some(ecc_req) => {
                // check if the valid ecc request type is received from the module.
                if ecc_req.ecc_info.req != EccRequestType::VerifySignature
                    && ecc_req.ecc_info.req != EccRequestType::GenerateSignature
                {
                    error!("Received ECC request type is invalid for this operation.");
                    return Err(StackError::InvalidState);
                }

                Ok(self.manager.send_ecc_read_complete()?)
            }
            None => {
                error!("ECC request is not received from the module.");
                return Err(StackError::InvalidState);
            }
        }
    }

    /// Reads the ECDSA digest from the WINC module.
    ///
    /// The size of the digest or hash to be read can be determined from
    /// the `EccRequestType::GenerateSignature` request received from the module.
    ///
    /// # Arguments
    ///
    /// * `digest` - A mutable buffer to store the ECDSA digest.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the digest was successfully read.
    /// * `Err(StackError)` - If an error occurred while reading the digest.
    #[cfg(feature = "experimental-ecc")]
    pub fn ssl_read_ecdsa_digest(&mut self, digest: &mut [u8]) -> Result<(), StackError> {
        // check if ecc request is received from the module.
        match self.callbacks.ssl_cb_info.ecc_req.as_ref() {
            Some(ecc_req) => {
                // check if the valid ecc request type is received from the module.
                if ecc_req.ecc_info.req != EccRequestType::GenerateSignature {
                    error!(
                        "Received ECC request type is invalid for this operation. Expected: {:?}, got: {:?}.",
                        EccRequestType::GenerateSignature,
                        ecc_req.ecc_info.req
                    );
                    return Err(StackError::InvalidState);
                }

                // read the ECDSA signing digest.
                let ecc_reg = self
                    .callbacks
                    .ssl_cb_info
                    .ecc_req
                    .as_ref()
                    .map(|ecc_req| ecc_req.hif_reg)
                    .ok_or(StackError::InvalidState)?;

                Ok(self.manager.read_ecc_info(ecc_reg, digest)?)
            }
            None => {
                error!("ECC request is not received from the module.");
                return Err(StackError::InvalidState);
            }
        }
    }
}
