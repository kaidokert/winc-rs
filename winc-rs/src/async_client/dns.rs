use crate::net_ops::op::AsyncOp;
use crate::transfer::Xfer;
use crate::StackError;
use embedded_nal_async::AddrType;
use embedded_nal_async::Dns;

use super::AsyncClient;

impl<X: Xfer> Dns for AsyncClient<'_, X> {
    type Error = StackError;

    async fn get_host_by_name(
        &self,
        host: &str,
        addr_type: embedded_nal::AddrType,
    ) -> Result<core::net::IpAddr, Self::Error> {
        if addr_type != AddrType::IPv4 {
            unimplemented!("IPv6 not supported");
        }

        let dns_op = crate::net_ops::dns::DnsOp::new(host, Self::DNS_TIMEOUT)?;
        let mut async_dns_op = AsyncOp::new(dns_op, &self.manager, &self.callbacks, || {
            self.dispatch_events()
        });

        // NOTE: Direct .await still hangs in test environments because timeout mechanism
        // relies on poll-count decrementation, not time-based timeouts.
        // smol provides proper task scheduling, but without hardware events or timer-based
        // wakers, operations that only timeout on poll count can't complete with .await
        //
        // Future improvement: Use time-based timeouts like smol::Timer
        // For now, use manual polling for test compatibility:
        loop {
            match core::future::Future::poll(
                core::pin::Pin::new(&mut async_dns_op),
                &mut core::task::Context::from_waker(core::task::Waker::noop()),
            ) {
                core::task::Poll::Ready(result) => return result,
                core::task::Poll::Pending => {
                    self.yield_once().await;
                }
            }
        }
    }

    async fn get_host_by_address(
        &self,
        _addr: core::net::IpAddr,
        _result: &mut [u8],
    ) -> Result<usize, Self::Error> {
        unimplemented!("The Winc1500 stack does not support get_host_by_address()");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::manager::EventListener;
    use crate::stack::socket_callbacks::SocketCallbacks;
    use core::cell::RefCell;
    use core::net::{IpAddr, Ipv4Addr};

    use super::super::tests::make_test_client;
    use embedded_nal_async::Dns;
    use macro_rules_attribute::apply;
    use smol_macros::test;

    #[apply(test!)]
    async fn async_dns_timeout() {
        let client = make_test_client();
        let host = "www.google.com";
        let addr_type = embedded_nal::AddrType::IPv4;
        let result = client.get_host_by_name(host, addr_type).await;
        assert_eq!(result, Err(StackError::DnsTimeout));
    }

    #[apply(test!)]
    async fn async_dns_resolve() {
        let mut client = make_test_client();
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_resolve(Ipv4Addr::new(127, 0, 0, 1), "");
        };
        client.debug_callback = RefCell::new(Some(&mut my_debug));
        let result = client.get_host_by_name("example.com", AddrType::IPv4).await;
        assert_eq!(result, Ok(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
    }

    #[apply(test!)]
    async fn async_dns_resolve_failed() {
        let mut client = make_test_client();
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_resolve(Ipv4Addr::new(0, 0, 0, 0), "");
        };
        client.debug_callback = RefCell::new(Some(&mut my_debug));
        let result = client
            .get_host_by_name("nonexistent.com", AddrType::IPv4)
            .await;
        assert_eq!(result, Err(StackError::DnsFailed));
    }
}
