use core::net::{SocketAddr, SocketAddrV4};

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
    initialized: bool,
    from_addr: Option<SocketAddrV4>,
}

impl<'buffer> UdpReceiveOp<'buffer> {
    pub fn new(handle: Handle, buffer: &'buffer mut [u8]) -> Self {
        Self {
            handle,
            buffer,
            initialized: false,
            from_addr: None,
        }
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
        // Check if we have a previous operation with remaining data first
        if let Some((_sock, op)) = callbacks.udp_sockets.get(self.handle) {
            if let ClientSocketOp::AsyncOp(
                AsyncOp::RecvFrom(Some(ref mut recv_result)),
                AsyncState::Done,
            ) = op
            {
                if recv_result.return_offset < recv_result.recv_len {
                    let remaining_data = recv_result.recv_len - recv_result.return_offset;
                    let copy_len = remaining_data.min(self.buffer.len());

                    // Copy remaining data from recv_buffer
                    self.buffer[..copy_len].copy_from_slice(
                        &callbacks.recv_buffer
                            [recv_result.return_offset..recv_result.return_offset + copy_len],
                    );

                    recv_result.return_offset += copy_len;
                    let from_addr = recv_result.from_addr;

                    // Clear operation if all data consumed
                    if recv_result.return_offset >= recv_result.recv_len {
                        *op = ClientSocketOp::None;
                    }

                    return Ok(Some((copy_len, SocketAddr::V4(from_addr))));
                }
            }
        }

        // Initialize receive request if not done yet
        if !self.initialized {
            if let Some((sock, op)) = callbacks.udp_sockets.get(self.handle) {
                let socket = *sock;

                manager
                    .send_recvfrom(socket, socket.get_recv_timeout())
                    .map_err(StackError::ReceiveFailed)?;

                *op = ClientSocketOp::AsyncOp(AsyncOp::RecvFrom(None), AsyncState::Pending(None));

                self.initialized = true;
                return Ok(None); // Operation started, still in progress
            } else {
                return Err(StackError::SocketNotFound);
            }
        }

        // Check if receive operation is complete
        if let Some((sock, op)) = callbacks.udp_sockets.get(self.handle) {
            let socket = *sock;

            match op {
                ClientSocketOp::AsyncOp(AsyncOp::RecvFrom(Some(recv_result)), _) => {
                    match recv_result.error {
                        SocketError::NoError => {
                            let recv_len = recv_result.recv_len;
                            let from_addr = recv_result.from_addr;
                            self.from_addr = Some(from_addr);

                            if recv_len == 0 {
                                // No data received - keep trying
                                return Ok(None);
                            } else {
                                let copy_len = recv_len.min(self.buffer.len());

                                // Copy data to user buffer
                                self.buffer[..copy_len]
                                    .copy_from_slice(&callbacks.recv_buffer[..copy_len]);
                                recv_result.return_offset = copy_len;

                                if copy_len < recv_len {
                                    // Partial read - leave operation for next call
                                    // Don't clear the operation
                                } else {
                                    // Complete read - clear operation
                                    *op = ClientSocketOp::None;
                                }

                                return Ok(Some((copy_len, SocketAddr::V4(from_addr))));
                            }
                        }
                        SocketError::Timeout => {
                            // Retry on timeout
                            manager
                                .send_recvfrom(socket, socket.get_recv_timeout())
                                .map_err(StackError::ReceiveFailed)?;

                            *op = ClientSocketOp::AsyncOp(
                                AsyncOp::RecvFrom(None),
                                AsyncState::Pending(None),
                            );

                            return Ok(None); // Continue waiting
                        }
                        error => {
                            // Clear operation on error
                            *op = ClientSocketOp::None;
                            return Err(StackError::OpFailed(error));
                        }
                    }
                }
                _ => {
                    // No relevant operation or wrong state
                }
            }
        }

        // Still waiting for completion
        Ok(None)
    }
}
