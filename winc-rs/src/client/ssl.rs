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
use crate::{error, warn};

#[cfg(feature = "experimental-ecc")]
use crate::manager::{EccInfo, EccPoint, EccRequestType, EcdhInfo, EcdsaSignInfo};

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
    pub fn ssl_set_cipher_suite(
        &mut self,
        ssl_cipher: SslCipherSuite,
    ) -> nb::Result<(), StackError> {
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
    /// * `ecdh_info` – An optional reference to the ECDH information structure.
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
        ecdh_info: Option<&EcdhInfo>,
        resp_buffer: &[u8],
    ) -> Result<(), StackError> {
        // clear the previously acquired ECC HIF register.
        if let Some(ecc_req) = self.callbacks.ssl_cb_info.ecc_req.as_mut() {
            ecc_req.hif_reg = 0;
        } else {
            // no ECC request is received from the module.
            return Err(StackError::InvalidState);
        }

        Ok(self
            .manager
            .send_ecc_resp(ecc_info, ecdh_info, resp_buffer)?)
    }

    /// Reads the SSL certificate from the WINC module.
    ///
    /// This function attempts to read the certificate only when an ECC request of type
    /// `EccRequestType::VerifySignature` is received from the WINC module.
    ///
    /// # Arguments
    ///
    /// * `ecdsa_info` – A mutable reference to store ECDSA information (curve type and hash size).
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
        ecdsa_info: &mut EcdsaSignInfo,
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
                ecdsa_info.curve_type = u16::from_be_bytes([opts[0], opts[1]]).into();
                ecc_point.point_size = u16::from_be_bytes([opts[2], opts[3]]);
                ecdsa_info.hash_size = u16::from_be_bytes([opts[4], opts[5]]);
                let sig_size = u16::from_be_bytes([opts[6], opts[7]]);

                let warn_truncated = |label: &str, expected: usize, available: usize| {
                    if available < expected {
                        warn!(
                            "{} read truncated: expected {} bytes, but only {} bytes available in buffer",
                            label, expected, available
                        );
                    }
                };

                // Read the ECC Point-X
                let expected_size = (ecc_point.point_size) as usize;
                let mut to_read = ecc_point.x_pos.len().min(expected_size);

                warn_truncated("ECC x-coordinates", expected_size, to_read);

                self.manager
                    .read_ecc_info(hif_addr, &mut ecc_point.x_pos[..to_read])?;
                hif_addr += (ecc_point.point_size) as u32;

                // Read the ECC Point-Y
                to_read = ecc_point.y_pos.len().min(expected_size);

                warn_truncated("ECC y-coordinates", expected_size, to_read);

                self.manager
                    .read_ecc_info(hif_addr, &mut ecc_point.y_pos[..to_read])?;
                hif_addr += (ecc_point.point_size) as u32;

                // Read the hash
                to_read = hash.len().min(ecdsa_info.hash_size as usize);

                warn_truncated("Hash", ecdsa_info.hash_size as usize, to_read);

                self.manager
                    .read_ecc_info(hif_addr, &mut hash[..to_read as usize])?;
                hif_addr += ecdsa_info.hash_size as u32;

                // Read the Signature
                to_read = signature.len().min(sig_size as usize);

                warn_truncated("Signature", sig_size as usize, to_read);

                self.manager
                    .read_ecc_info(hif_addr, &mut signature[..to_read])?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        client::{test_shared::*, SocketCallbacks},
        manager::{EventListener, SslResponse},
    };

    #[cfg(feature = "experimental-ecc")]
    use crate::manager::EccRequest;

    #[test]
    fn test_ssl_set_cipher_suit_success() {
        let mut client = make_test_client();
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_ssl(
                SslResponse::CipherSuiteUpdate,
                Some(SslCipherSuite::AllCiphers.into()),
                #[cfg(feature = "experimental-ecc")]
                None,
            );
        };
        client.debug_callback = Some(&mut my_debug);
        let result = nb::block!(client.ssl_set_cipher_suite(SslCipherSuite::AllCiphers));
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_ssl_set_cipher_suit_failure() {
        let mut client = make_test_client();
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_ssl(
                SslResponse::CipherSuiteUpdate,
                Some(SslCipherSuite::DheRsaWithAes128CbcSha.into()),
                #[cfg(feature = "experimental-ecc")]
                None,
            );
        };
        client.debug_callback = Some(&mut my_debug);
        let result = nb::block!(client.ssl_set_cipher_suite(SslCipherSuite::AllCiphers));
        assert_eq!(result, Err(StackError::InvalidResponse));
    }

    #[test]
    fn test_ssl_set_cipher_suit_timeout() {
        let mut client = make_test_client();
        client.callbacks.ssl_cb_info.cipher_suite_bitmap = None;
        let result = nb::block!(client.ssl_set_cipher_suite(SslCipherSuite::AllCiphers));
        assert_eq!(result, Err(StackError::GeneralTimeout));
    }

    #[test]
    fn test_ssl_set_cert_expiry_success() {
        let mut client = make_test_client();

        let result = client.ssl_check_cert_expiry(SslCertExpiryOpt::Enabled);

        assert_eq!(result, Ok(()));
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_send_ecc_resp_success() {
        let mut client = make_test_client();

        client.callbacks.ssl_cb_info.ecc_req = Some(EccRequest::default());

        let mut ecc_info = EccInfo::default();
        let ecdh_info = EcdhInfo::default();
        ecc_info.req = EccRequestType::ClientEcdh;

        let result = client.ssl_send_ecc_resp(&ecc_info, Some(&ecdh_info), &[0]);

        assert_eq!(result, Ok(()))
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_send_ecc_resp_fail() {
        let mut client = make_test_client();

        client.callbacks.ssl_cb_info.ecc_req = None;

        let mut ecc_info = EccInfo::default();
        let ecdh_info = EcdhInfo::default();
        ecc_info.req = EccRequestType::ServerEcdh;

        let result = client.ssl_send_ecc_resp(&ecc_info, Some(&ecdh_info), &[0]);

        assert_eq!(result, Err(StackError::InvalidState));
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_send_ecc_resp_hif_reg_override() {
        let mut client = make_test_client();

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_ssl(
                SslResponse::EccReqUpdate,
                None,
                Some(EccRequest {
                    hif_reg: 15,
                    ecc_info: EccInfo::default(),
                    ecdh_info: None,
                    ecdsa_sign_info: None,
                    ecdsa_verify_info: None,
                }),
            );
        };

        client.debug_callback = Some(&mut my_debug);

        let result = client.dispatch_events_may_wait();
        assert!(result.is_ok());

        let mut ecc_info = EccInfo::default();
        let ecdh_info = EcdhInfo::default();

        ecc_info.req = EccRequestType::GenerateKey;

        let result = client.ssl_send_ecc_resp(&ecc_info, Some(&ecdh_info), &[0]);
        let reg = client
            .callbacks
            .ssl_cb_info
            .ecc_req
            .as_ref()
            .map(|ecc_req| ecc_req.hif_reg);

        assert_eq!(result, Ok(()));

        assert_eq!(reg, Some(0));
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_read_cert_success() {
        let mut client = make_test_client();
        let mut ecc_req = EccRequest::default();
        let mut ecdsa_info = EcdsaSignInfo::default();
        let mut hash = [0u8; 10];
        let mut sign = [0u8; 10];
        let mut ecc_point = EccPoint::default();

        ecc_req.ecc_info.req = EccRequestType::VerifySignature;
        client.callbacks.ssl_cb_info.ecc_req = Some(ecc_req);

        let result =
            client.ssl_read_certificate(&mut ecdsa_info, &mut hash, &mut sign, &mut ecc_point);

        assert_eq!(result, Ok(()));
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_read_cert_invalid_ecc_request() {
        let mut client = make_test_client();
        let mut ecc_req = EccRequest::default();
        let mut ecdsa_info = EcdsaSignInfo::default();
        let mut hash = [0u8; 10];
        let mut sign = [0u8; 10];
        let mut ecc_point = EccPoint::default();

        ecc_req.ecc_info.req = EccRequestType::GenerateSignature;
        client.callbacks.ssl_cb_info.ecc_req = Some(ecc_req);

        let result =
            client.ssl_read_certificate(&mut ecdsa_info, &mut hash, &mut sign, &mut ecc_point);

        assert_eq!(result, Err(StackError::InvalidState));
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_read_cert_invalid_state() {
        let mut client = make_test_client();
        let mut ecdsa_info = EcdsaSignInfo::default();
        let mut hash = [0u8; 10];
        let mut sign = [0u8; 10];
        let mut ecc_point = EccPoint::default();

        client.callbacks.ssl_cb_info.ecc_req = None;

        let result =
            client.ssl_read_certificate(&mut ecdsa_info, &mut hash, &mut sign, &mut ecc_point);

        assert_eq!(result, Err(StackError::InvalidState));
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_clear_ecc_read_success() {
        let mut client = make_test_client();
        let mut ecc_req = EccRequest::default();

        ecc_req.ecc_info.req = EccRequestType::GenerateSignature;
        client.callbacks.ssl_cb_info.ecc_req = Some(ecc_req);

        let result = client.ssl_clear_ecc_readable();

        assert_eq!(result, Ok(()));
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_clear_ecc_read_ok_req() {
        let mut client = make_test_client();
        let mut ecc_req = EccRequest::default();

        ecc_req.ecc_info.req = EccRequestType::VerifySignature;
        client.callbacks.ssl_cb_info.ecc_req = Some(ecc_req);

        let result = client.ssl_clear_ecc_readable();

        assert_eq!(result, Ok(()));
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_clear_ecc_read_invalid_request() {
        let mut client = make_test_client();
        let mut ecc_req = EccRequest::default();

        ecc_req.ecc_info.req = EccRequestType::ClientEcdh;
        client.callbacks.ssl_cb_info.ecc_req = Some(ecc_req);

        let result = client.ssl_clear_ecc_readable();

        assert_eq!(result, Err(StackError::InvalidState));
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_clear_ecc_read_fail() {
        let mut client = make_test_client();

        client.callbacks.ssl_cb_info.ecc_req = None;

        let result = client.ssl_clear_ecc_readable();

        assert_eq!(result, Err(StackError::InvalidState));
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_read_ecc_digest_success() {
        let mut client = make_test_client();
        let mut ecc_req = EccRequest::default();
        let mut digest = [0u8; 10];

        ecc_req.ecc_info.req = EccRequestType::GenerateSignature;
        client.callbacks.ssl_cb_info.ecc_req = Some(ecc_req);

        let result = client.ssl_read_ecdsa_digest(&mut digest);

        assert_eq!(result, Ok(()));
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_read_ecc_digest_invalid_request() {
        let mut client = make_test_client();
        let mut ecc_req = EccRequest::default();
        let mut digest = [0u8; 10];

        ecc_req.ecc_info.req = EccRequestType::VerifySignature;
        client.callbacks.ssl_cb_info.ecc_req = Some(ecc_req);

        let result = client.ssl_read_ecdsa_digest(&mut digest);

        assert_eq!(result, Err(StackError::InvalidState));
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_read_ecc_digest_fail() {
        let mut client = make_test_client();
        let mut digest = [0u8; 10];

        client.callbacks.ssl_cb_info.ecc_req = None;

        let result = client.ssl_read_ecdsa_digest(&mut digest);

        assert_eq!(result, Err(StackError::InvalidState));
    }
}
