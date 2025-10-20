use crate::net_ops::op::AsyncOp;
use crate::net_ops::udp_receive::UdpReceiveOp;
use crate::net_ops::udp_send::UdpSendOp;
use crate::stack::sock_holder::SocketStore;
use crate::transfer::Xfer;
use crate::Handle;
use crate::StackError;
use embedded_nal_async::UnconnectedUdp;

use super::AsyncClient;

impl<X: Xfer> AsyncClient<'_, X> {
    /// Get or create the UDP socket for UnconnectedUdp operations
    fn get_or_create_udp_socket(&self) -> Result<Handle, StackError> {
        let mut socket_opt = self.udp_socket.borrow_mut();

        if let Some(handle) = *socket_opt {
            // Socket already exists, return it
            crate::debug!("Reusing existing UDP socket: {:?}", handle);
            Ok(handle)
        } else {
            // Create new socket
            let session_id = self.get_next_session_id();
            let mut callbacks = self.callbacks.borrow_mut();
            let handle = callbacks
                .udp_sockets
                .add(session_id)
                .ok_or(StackError::OutOfSockets)?;

            crate::debug!("Created new UDP socket: {:?}", handle);

            // Store it for reuse
            *socket_opt = Some(handle);
            Ok(handle)
        }
    }

    /// Close the UDP socket if it exists
    fn close_udp_socket(&self) {
        let mut socket_opt = self.udp_socket.borrow_mut();

        if let Some(handle) = socket_opt.take() {
            let mut manager = self.manager.borrow_mut();
            let mut callbacks = self.callbacks.borrow_mut();

            if let Some((sock, _op)) = callbacks.udp_sockets.get(handle) {
                let _ = manager.send_close(*sock);
                callbacks.udp_sockets.remove(handle);
            }
        }
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
        crate::debug!(
            "AsyncClient::send called - local: {:?}, remote: {:?}, data_len: {}",
            local,
            remote,
            data.len()
        );

        // Convert to IPv4 addresses (IPv6 not supported)
        let _local_v4 = match local {
            core::net::SocketAddr::V4(addr) => addr,
            core::net::SocketAddr::V6(_) => return Err(StackError::InvalidParameters),
        };

        let remote_v4 = match remote {
            core::net::SocketAddr::V4(addr) => addr,
            core::net::SocketAddr::V6(_) => return Err(StackError::InvalidParameters),
        };

        // Get or create UDP socket (reused across send/receive)
        let handle = self.get_or_create_udp_socket()?;
        crate::debug!("Got UDP socket handle: {:?}", handle);

        // Create UDP send operation
        let udp_send_op = UdpSendOp::new(handle, remote_v4, data);
        crate::debug!("Created UdpSendOp, starting async operation");

        // Create async operation wrapper
        let async_udp_send = AsyncOp::new(udp_send_op, &self.manager, &self.callbacks, || {
            self.dispatch_events()
        });

        // Await completion
        crate::debug!("Awaiting UDP send completion");
        let result = async_udp_send.await;
        crate::debug!("UDP send completed with result: {:?}", result);
        result
    }

    async fn receive_into(
        &mut self,
        buffer: &mut [u8],
    ) -> Result<(usize, core::net::SocketAddr, core::net::SocketAddr), Self::Error> {
        // Get or create UDP socket (reused from send)
        let handle = self.get_or_create_udp_socket()?;

        // Create UDP receive operation
        let udp_receive_op = UdpReceiveOp::new(handle, buffer);

        // Create async operation wrapper
        let async_udp_receive =
            AsyncOp::new(udp_receive_op, &self.manager, &self.callbacks, || {
                self.dispatch_events()
            });

        // Await completion
        let result = async_udp_receive.await;

        // Process result
        match result {
            Ok((len, remote_addr)) => {
                // For UnconnectedUdp, we need to return (len, local, remote)
                // We could track the actual local address used, but for now return UNSPECIFIED
                let local_addr = core::net::SocketAddr::V4(core::net::SocketAddrV4::new(
                    core::net::Ipv4Addr::UNSPECIFIED,
                    0,
                ));
                Ok((len, local_addr, remote_addr))
            }
            Err(e) => Err(e),
        }
    }
}

impl<X: Xfer> Drop for AsyncClient<'_, X> {
    fn drop(&mut self) {
        // Clean up UDP socket when AsyncClient is dropped
        self.close_udp_socket();
    }
}
