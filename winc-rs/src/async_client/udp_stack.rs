use crate::net_ops::op::AsyncOp;
use crate::net_ops::udp_receive::UdpReceiveOp;
use crate::net_ops::udp_send::UdpSendOp;
use crate::transfer::Xfer;
use crate::Handle;
use crate::StackError;
use embedded_nal_async::UnconnectedUdp;

use super::AsyncClient;

impl embedded_io_async::Error for StackError {
    fn kind(&self) -> embedded_io_async::ErrorKind {
        embedded_io_async::ErrorKind::Other
    }
}

impl<X: Xfer> UnconnectedUdp for AsyncClient<'_, X> {
    type Error = StackError;

    async fn send(
        &mut self,
        local: core::net::SocketAddr,
        remote: core::net::SocketAddr,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        // Convert to IPv4 addresses (IPv6 not supported)
        let _local_v4 = match local {
            core::net::SocketAddr::V4(addr) => addr,
            core::net::SocketAddr::V6(_) => return Err(StackError::InvalidParameters),
        };

        let remote_v4 = match remote {
            core::net::SocketAddr::V4(addr) => addr,
            core::net::SocketAddr::V6(_) => return Err(StackError::InvalidParameters),
        };

        // Create a UDP socket for this send operation
        // Note: This is a simplified implementation - a real implementation would
        // need proper socket management
        // TODO: This is placeholder code.
        let handle = Handle(7); // Placeholder handle

        // Create UDP send operation
        let udp_send_op = UdpSendOp::new(handle, remote_v4, data);

        // Create async operation wrapper
        let async_udp_send = AsyncOp::new(udp_send_op, &self.manager, &self.callbacks, || {
            self.dispatch_events()
        });

        // Await completion - the runtime's waker will drive progress
        async_udp_send.await
    }

    async fn receive_into(
        &mut self,
        buffer: &mut [u8],
    ) -> Result<(usize, core::net::SocketAddr, core::net::SocketAddr), Self::Error> {
        // Create a UDP socket for this receive operation
        // Note: This is a simplified implementation - a real implementation would
        // need proper socket management
        // TODO: This is placeholder code.
        let handle = Handle(7); // Placeholder handle

        // Create UDP receive operation
        let udp_receive_op = UdpReceiveOp::new(handle, buffer);

        // Create async operation wrapper
        let async_udp_receive =
            AsyncOp::new(udp_receive_op, &self.manager, &self.callbacks, || {
                self.dispatch_events()
            });

        // Await completion - the runtime's waker will drive progress
        match async_udp_receive.await {
            Ok((len, remote_addr)) => {
                // For UnconnectedUdp, we need to return (len, local, remote)
                // but we don't track the local address properly in this simplified impl
                let local_addr = core::net::SocketAddr::V4(core::net::SocketAddrV4::new(
                    core::net::Ipv4Addr::new(0, 0, 0, 0),
                    0,
                ));
                Ok((len, local_addr, remote_addr))
            }
            Err(e) => Err(e),
        }
    }
}
