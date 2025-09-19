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

use super::StackError;
use super::WincClient;
use super::Xfer;

use crate::manager::{EccPoint, EccRequest, SslCertExpiryOpt};

// @note: The `m2m_ssl_retrieve_hash` API is not supported because
// there is no way to validate that the SSL HIF register address
// received from the ECC response callback points to the memory
// region containing the certificate hash.

/// SSL Cipher Suits
/// By default, WINC1500 HW accelerator only supports AES-128.
/// For using, AES-256 needs to be enabled.
#[repr(u32)]
pub enum SslCipherSuite {
    // Individual Ciphers
    RsaWithAes128CbcSha = 0x01,
    RsaWithAes128CbcSha256 = 0x02,
    DheRsaWithAes128CbcSha = 0x04,
    DheRsaWithAes128CbcSha256 = 0x08,
    RsaWithAes128GcmSha256 = 0x10,
    DheRsaWithAes128GcmSha256 = 0x20,
    RsaWithAes256CbcSha = 0x40,
    RsaWithAes256CbcSha256 = 0x80,
    DheRsaWithAes256CbcSha = 0x100,
    DheRsaWithAes256CbcSha256 = 0x200,
    EcdheRsaWithAes128CbcSha = 0x400,
    EcdheRsaWithAes256CbcSha = 0x800,
    EcdheRsaWithAes128CbcSha256 = 0x1000,
    EcdheEcdsaWithAes128CbcSha256 = 0x2000,
    EcdheRsaWithAes128GcmSha256 = 0x4000,
    EcdheEcdsaWithAes128GcmSha256 = 0x8000,
    // Grouped Ciphers
    /// ECC ciphers using ECC authentication with AES 128 encryption only.
    /// By default, this group is disabled on startup.
    EccOnlyAes128 = 0xA000,
    /// ECC ciphers using any authentication with AES-128 encryption.
    /// By default, this group is disabled on startup.
    EccAllAes128 = 0xF400,
    /// All none ECC ciphers using AES-128 encryption.
    /// By default, this group is active on startup.
    NoEccAes128 = 0x3F,
    /// All none ECC ciphers using AES-256 encryption.
    NoEccAes256 = 0x3C0,
    /// All supported ciphers.
    /// By default, this group is disabled on startup.
    AllCiphers = 0xFFFF,
}

/// Implementation to convert `SslCipherSuite` to `u32`.
impl From<SslCipherSuite> for u32 {
    fn from(val: SslCipherSuite) -> Self {
        val as u32
    }
}

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

    /// Sends an SSL certificate to the module.
    ///
    /// # Arguments
    ///
    /// * `cert` – A byte slice containing the SSL certificate data.
    ///
    /// # Returns
    ///
    /// * `Ok(())` – If the certificate was successfully sent.
    /// * `Err(StackError)` – If an error occurred while sending the certificate.
    pub fn ssl_send_certificate(&mut self, cert: &[u8]) -> Result<(), StackError> {
        Ok(self.manager.send_ssl_cert(cert)?)
    }

    /// Sends an ECC response to module.
    ///
    /// # Arguments
    ///
    /// * `ecc_req` – ECC request structure.
    /// * `resp_buffer` - Buffer containing the ECC responses.
    ///
    /// # Returns
    ///
    /// * `Ok(())` – If the certificate was successfully sent.
    /// * `Err(StackError)` – If an error occurred while sending the certificate.
    pub fn ssl_send_ecc_resp(
        &mut self,
        ecc_req: &EccRequest,
        resp_buffer: &[u8],
    ) -> Result<(), StackError> {
        Ok(self.manager.send_ssl_ecc_resp(ecc_req, resp_buffer)?)
    }

    /// Reads the SSL response from the Module.
    ///
    /// # Arguments
    ///
    /// * `ecc_req` – ECC request structure.
    /// * `resp_buffer` - Buffer containing the ECC responses.
    ///
    /// # Returns
    ///
    /// * `Ok(())` – If the certificate was successfully sent.
    /// * `Err(StackError)` – If an error occurred while sending the certificate.
    pub fn ssl_read_certificate(
        &mut self,
        curve_type: &mut u16,
        hash: &mut [u8],
        signature: &mut [u8],
        ecc_point: &mut EccPoint,
    ) -> Result<(), StackError> {
        let mut hif_addr = self.callbacks.ssl_hif_reg.ok_or(StackError::InvalidState)?;
        let mut opts = [0u8; 8]; // read the ssl options.

        // Read the Curve Type, Key, Hash and Signature size.
        self.manager.read_ssl_cert_feat(hif_addr, &mut opts)?;
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
            .read_ssl_cert_feat(hif_addr, &mut ecc_point.x_cord[..to_read_len])?;
        hif_addr += to_read_len as u32;

        // Read the hash
        self.manager
            .read_ssl_cert_feat(hif_addr, &mut hash[..hash_size as usize])?;
        hif_addr += hash_size as u32;

        // Read the Signature
        self.manager
            .read_ssl_cert_feat(hif_addr, &mut signature[..sig_size as usize])?;

        // clear the Hif register
        self.callbacks.ssl_hif_reg = None;

        // mark the Rx done
        Ok(self.manager.send_ssl_cert_read_complete()?)
    }

    /// Sets the SSL/TLS cipher suite for the WINC module.
    ///
    /// # Arguments
    ///
    /// * `ssl_cipher` -  Required cipher suit to set.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the cipher suite was successfully set.
    /// * `Err(StackError)` - If an error occurred while applying the configuration.
    pub fn ssl_set_cipher_suit(&mut self, ssl_cipher: SslCipherSuite) -> Result<(), StackError> {
        Ok(self.manager.send_ssl_set_cipher_suit(ssl_cipher.into())?)
    }
}
