use core::net::IpAddr;
use core::net::Ipv4Addr;

use super::op::OpImpl;
use crate::transfer::Xfer;
use crate::StackError;

// Pure DNS operation state - no references, fully shareable
#[derive(Debug)]
pub struct DnsOp {
    host: heapless::String<63>, // MAX_HOST_NAME_LEN = 63
    count: u32,
    initialized: bool,
}

impl DnsOp {
    pub fn new(host: &str, timeout: u32) -> Result<Self, StackError> {
        let host_string =
            heapless::String::try_from(host).map_err(|_| StackError::InvalidParameters)?;

        Ok(Self {
            host: host_string,
            count: timeout,
            initialized: false,
        })
    }
}

impl<X: Xfer> OpImpl<X> for DnsOp {
    type Output = IpAddr;
    type Error = StackError;

    fn poll_impl(
        &mut self,
        manager: &mut crate::manager::Manager<X>,
        callbacks: &mut crate::stack::socket_callbacks::SocketCallbacks,
    ) -> Result<Option<Self::Output>, Self::Error> {
        // Initialize DNS request if not done yet
        if !self.initialized {
            // Only initialize if not already set (e.g., by test mock/debug callback)
            if callbacks.dns_resolved_addr.is_none() {
                callbacks.dns_resolved_addr = Some(None);
            }
            manager.send_gethostbyname(&self.host)?;
            self.initialized = true;
        }

        // Check if DNS resolution is complete
        if let Some(result) = &mut callbacks.dns_resolved_addr {
            if let Some(ip) = result.take() {
                callbacks.dns_resolved_addr = None;
                return if ip == Ipv4Addr::new(0, 0, 0, 0) {
                    Err(StackError::DnsFailed)
                } else {
                    Ok(Some(IpAddr::V4(ip)))
                };
            }
        }

        // Check timeout
        if self.count == 0 {
            return Err(StackError::DnsTimeout);
        }

        // Decrement count and return None (means WouldBlock)
        self.count -= 1;
        Ok(None)
    }
}
