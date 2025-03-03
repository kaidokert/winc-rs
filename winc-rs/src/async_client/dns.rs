use core::net::IpAddr;

use crate::client::GlobalOp;
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
        {
            let mut callbacks = self.callbacks.borrow_mut();
            callbacks.last_recv_addr = None;
            // Todo: this global_op isn't necessary at all, just use `last_recv_addr` for signaling
            callbacks.global_op = Some(GlobalOp::GetHostByName);
        }
        {
            let mut manager = self.manager.borrow_mut();
            manager
                .send_gethostbyname(host)
                .map_err(|_x| StackError::GlobalOpFailed)?;
        }
        let mut count = Self::DNS_TIMEOUT;
        loop {
            // todo: make this async so we can simply .await on it
            self.dispatch_events()
                .map_err(|_x| StackError::GlobalOpFailed)?;
            {
                // The callbacks system CLEARS GlobalOpn
                let mut callbacks = self.callbacks.borrow_mut();
                if callbacks.global_op.is_none() {
                    if let Some(addr) = callbacks.last_recv_addr.take() {
                        return Ok(IpAddr::V4(*addr.ip()));
                    }
                }
            }
            self.yield_once().await;
            count -= 1;
            if count == 0 {
                return Err(StackError::DnsTimeout);
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
    use core::net::Ipv4Addr;

    use super::super::tests::make_test_client;
    use embedded_nal_async::Dns;

    #[async_std::test]
    async fn async_dns_timeout() {
        let client = make_test_client();
        let host = "www.google.com";
        let addr_type = embedded_nal::AddrType::IPv4;
        let result = client.get_host_by_name(host, addr_type).await;
        assert_eq!(result, Err(StackError::DnsTimeout));
    }

    #[async_std::test]
    async fn async_dns_resolve() {
        let mut client = make_test_client();
        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_resolve(Ipv4Addr::new(127, 0, 0, 1), "");
        };
        client.debug_callback = RefCell::new(Some(&mut my_debug));
        let result = client.get_host_by_name("example.com", AddrType::IPv4).await;
        assert_eq!(result, Ok(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
    }
}
