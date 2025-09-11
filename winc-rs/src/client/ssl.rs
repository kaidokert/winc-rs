use super::StackError;
use super::WincClient;
use super::Xfer;

use crate::manager::{SslCertExpiryOpt, EccReqInfo};

impl<X: Xfer> WincClient<'_, X> {
    /// Configure the SSL certificate expiry option.
    ///
    /// # Arguments
    ///
    /// * `opt` – The SSL certificate expiry option to apply.
    ///
    /// # Returns
    ///
    /// * `()` – If the request was successfully processed.
    /// * `StackError` – If an error occurred while configuring the option.
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
    /// * `()` – If the certificate was successfully sent.
    /// * `StackError` – If an error occurred while sending the certificate.
    pub fn ssl_send_certificate(&mut self, cert: &[u8]) -> Result<(), StackError> {
        Ok(self.manager.send_ssl_cert(cert)?)
    }

    pub fn ssl_handshake_resp(&mut self, ecc_req: &EccReqInfo, resp_buffer: &[u8]) -> Result<(), StackError> {
        
    }
}
