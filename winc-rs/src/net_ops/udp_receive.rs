use core::net::SocketAddr;

use super::op::OpImpl;
use crate::client::ClientSocketOp;
use crate::manager::SocketError;
use crate::stack::sock_holder::SocketStore;
use crate::stack::socket_callbacks::SocketCallbacks;
use crate::stack::socket_callbacks::{AsyncOp, AsyncState};
use crate::transfer::Xfer;
use crate::Handle;
use crate::StackError;

// Pure UDP receive operation state
#[derive(Debug)]
pub struct UdpReceiveOp<'buffer> {
    handle: Handle,
    buffer: &'buffer mut [u8],
}

impl<'buffer> UdpReceiveOp<'buffer> {
    pub fn new(handle: Handle, buffer: &'buffer mut [u8]) -> Self {
        Self { handle, buffer }
    }
}

impl<X: Xfer> OpImpl<X> for UdpReceiveOp<'_> {
    type Output = (usize, SocketAddr);
    type Error = StackError;

    fn poll_impl(
        &mut self,
        manager: &mut crate::manager::Manager<X>,
        callbacks: &mut SocketCallbacks,
    ) -> Result<Option<Self::Output>, Self::Error> {
        let (sock, op) = callbacks
            .udp_sockets
            .get(self.handle)
            .ok_or(StackError::SocketNotFound)?;
        let socket = *sock;

        // Handle partial data from a previous operation first
        if let ClientSocketOp::AsyncOp(
            AsyncOp::RecvFrom(Some(ref mut recv_result)),
            AsyncState::Done,
        ) = op
        {
            if recv_result.return_offset < recv_result.recv_len {
                let remaining_data = recv_result.recv_len - recv_result.return_offset;
                let copy_len = remaining_data.min(self.buffer.len());
                self.buffer[..copy_len].copy_from_slice(
                    &callbacks.recv_buffer
                        [recv_result.return_offset..recv_result.return_offset + copy_len],
                );
                recv_result.return_offset += copy_len;
                let from_addr = recv_result.from_addr;
                if recv_result.return_offset >= recv_result.recv_len {
                    *op = ClientSocketOp::None;
                }
                return Ok(Some((copy_len, SocketAddr::V4(from_addr))));
            }
        }

        match op {
            ClientSocketOp::AsyncOp(AsyncOp::RecvFrom(Some(recv_result)), _) => {
                match recv_result.error {
                    SocketError::NoError => {
                        let recv_len = recv_result.recv_len;
                        let from_addr = recv_result.from_addr;
                        if recv_len == 0 {
                            return Ok(None);
                        }
                        let copy_len = recv_len.min(self.buffer.len());
                        self.buffer[..copy_len].copy_from_slice(&callbacks.recv_buffer[..copy_len]);
                        recv_result.return_offset = copy_len;
                        if copy_len >= recv_len {
                            *op = ClientSocketOp::None;
                        }
                        Ok(Some((copy_len, SocketAddr::V4(from_addr))))
                    }
                    SocketError::Timeout => {
                        manager
                            .send_recvfrom(socket, socket.get_recv_timeout())
                            .map_err(StackError::ReceiveFailed)?;
                        *op = ClientSocketOp::AsyncOp(
                            AsyncOp::RecvFrom(None),
                            AsyncState::Pending(None),
                        );
                        Ok(None)
                    }
                    error => {
                        *op = ClientSocketOp::None;
                        Err(StackError::OpFailed(error))
                    }
                }
            }
            ClientSocketOp::AsyncOp(AsyncOp::RecvFrom(None), AsyncState::Pending(_)) => Ok(None),
            _ => {
                // Not initialized, so start the operation
                manager
                    .send_recvfrom(socket, socket.get_recv_timeout())
                    .map_err(StackError::ReceiveFailed)?;
                *op = ClientSocketOp::AsyncOp(AsyncOp::RecvFrom(None), AsyncState::Pending(None));
                Ok(None)
            }
        }
    }
}
