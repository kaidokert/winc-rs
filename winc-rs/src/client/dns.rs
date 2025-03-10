use core::net::IpAddr;
use core::net::Ipv4Addr;
use embedded_nal::nb;
use embedded_nal::AddrType;
use embedded_nal::Dns;

use super::StackError;
use crate::transfer::Xfer;
use crate::WincClient;

impl<X: Xfer> Dns for WincClient<'_, X> {
    type Error = StackError;

    fn get_host_by_name(
        &mut self,
        hostname: &str,
        addr_type: AddrType,
    ) -> embedded_nal::nb::Result<IpAddr, Self::Error> {
        match &mut self.callbacks.dns_resolved_addr {
            None => {
                if addr_type != AddrType::IPv4 {
                    unimplemented!("IPv6 not supported");
                }
                self.dispatch_events()?;
                self.manager
                    .send_gethostbyname(hostname)
                    .map_err(StackError::WincWifiFail)?;
                // Signal operation in progress
                self.operation_countdown = Self::DNS_TIMEOUT;
                self.callbacks.dns_resolved_addr = Some(None);
            }
            Some(result) => {
                if let Some(ip) = result.take() {
                    self.callbacks.dns_resolved_addr = None;
                    if ip == Ipv4Addr::new(0, 0, 0, 0) {
                        return Err(StackError::DnsFailed.into());
                    }
                    return Ok(IpAddr::V4(ip));
                }
                self.operation_countdown -= 1;
                if self.operation_countdown == 0 {
                    return Err(nb::Error::Other(StackError::DnsTimeout));
                }
            }
        }
        self.dispatch_events()?;
        Err(nb::Error::WouldBlock)
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
        let mut delay = |_| {};
        let mut client = make_test_client(&mut delay);
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_resolve(Ipv4Addr::new(127, 0, 0, 1), "");
        };
        client.debug_callback = Some(&mut my_debug);
        let result = nb::block!(client.get_host_by_name("example.com", AddrType::IPv4));
        assert_eq!(result.ok(), Some(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
    }
    #[test]
    fn test_get_host_by_name_timeout() {
        let mut delay = |_| {};
        let mut client = make_test_client(&mut delay);
        let result = nb::block!(client.get_host_by_name("example.com", AddrType::IPv4));
        assert_eq!(result.err(), Some(StackError::DnsTimeout.into()));
    }
    #[test]
    fn test_get_host_by_name_failed() {
        let mut delay = |_| {};
        let mut client = make_test_client(&mut delay);
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
        let mut delay = |_| {};
        let mut client = make_test_client(&mut delay);
        let _ = client.get_host_by_name("example.com", AddrType::IPv6);
    }

    #[test]
    #[should_panic]
    fn test_get_host_by_address() {
        let mut delay = |_| {};
        let mut client = make_test_client(&mut delay);
        let _ = client.get_host_by_address(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), &mut [0; 4]);
    }
}
