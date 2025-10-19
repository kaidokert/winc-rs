use core::net::IpAddr;
use embedded_nal::nb;
use embedded_nal::AddrType;
use embedded_nal::Dns;

use super::StackError;
use crate::net_ops::dns::DnsOp;
use crate::net_ops::op::OpImpl;
use crate::transfer::Xfer;
use crate::WincClient;

impl<X: Xfer> Dns for WincClient<'_, X> {
    type Error = StackError;

    fn get_host_by_name(
        &mut self,
        hostname: &str,
        addr_type: AddrType,
    ) -> embedded_nal::nb::Result<IpAddr, Self::Error> {
        if addr_type != AddrType::IPv4 {
            unimplemented!("IPv6 not supported");
        }

        // Initialize DNS op if not already started
        if self.dns_op.is_none() {
            match DnsOp::new(hostname, Self::DNS_TIMEOUT) {
                Ok(dns_op) => self.dns_op = Some(dns_op),
                Err(e) => return Err(nb::Error::Other(e)),
            }
        }

        // Handle test debug callback
        #[cfg(test)]
        {
            if let Some(callback) = &mut self.debug_callback {
                callback(&mut self.callbacks);
            }
        }

        // Dispatch events first
        self.dispatch_events()?;

        // Poll the DNS operation using the trait
        if let Some(dns_op) = &mut self.dns_op {
            match dns_op.poll_impl(&mut self.manager, &mut self.callbacks) {
                Ok(Some(ip)) => {
                    self.dns_op = None; // Clear the operation
                    Ok(ip)
                }
                Ok(None) => Err(nb::Error::WouldBlock),
                Err(e) => {
                    self.dns_op = None; // Clear the operation on error
                    Err(nb::Error::Other(e))
                }
            }
        } else {
            Err(nb::Error::WouldBlock)
        }
    }

    fn get_host_by_address(
        &mut self,
        _addr: core::net::IpAddr,
        _result: &mut [u8],
    ) -> Result<usize, embedded_nal::nb::Error<<Self as Dns>::Error>> {
        unimplemented!("The Winc1500 stack does not support get_host_by_address()");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{client::SocketCallbacks, manager::EventListener};
    use core::net::Ipv4Addr;

    use crate::client::test_shared::*;
    use embedded_nal::Dns;

    #[test]
    fn test_get_host_by_name_success() {
        let mut client = make_test_client();
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_resolve(Ipv4Addr::new(127, 0, 0, 1), "");
        };
        client.debug_callback = Some(&mut my_debug);
        let result = nb::block!(client.get_host_by_name("example.com", AddrType::IPv4));
        assert_eq!(result.ok(), Some(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
    }
    #[test]
    fn test_get_host_by_name_timeout() {
        let mut client = make_test_client();
        let result = nb::block!(client.get_host_by_name("example.com", AddrType::IPv4));
        assert_eq!(result.err(), Some(StackError::DnsTimeout.into()));
    }
    #[test]
    fn test_get_host_by_name_failed() {
        let mut client = make_test_client();
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_resolve(Ipv4Addr::new(0, 0, 0, 0), "");
        };
        client.debug_callback = Some(&mut my_debug);
        let result = nb::block!(client.get_host_by_name("nonexistent.com", AddrType::IPv4));
        assert_eq!(result.err(), Some(StackError::DnsFailed.into()));
    }

    #[test]
    #[should_panic]
    fn test_get_host_by_name_unsupported_addr_type() {
        let mut client = make_test_client();
        let _ = client.get_host_by_name("example.com", AddrType::IPv6);
    }

    #[test]
    #[should_panic]
    fn test_get_host_by_address() {
        let mut client = make_test_client();
        let _ = client.get_host_by_address(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), &mut [0; 4]);
    }
}
