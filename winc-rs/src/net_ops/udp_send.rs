use core::net::SocketAddrV4;

use super::op::OpImpl;
use crate::client::ClientSocketOp;
use crate::stack::constants::MAX_SEND_LENGTH;
use crate::stack::sock_holder::SocketStore;
use crate::stack::socket_callbacks::SocketCallbacks;
use crate::stack::socket_callbacks::{AsyncOp, AsyncState, SendRequest};
use crate::transfer::Xfer;
use crate::Handle;
use crate::StackError;

// Pure UDP send operation state - no references, fully shareable
#[derive(Debug)]
pub struct UdpSendOp<'data> {
    handle: Handle,
    addr: SocketAddrV4,
    data: &'data [u8],
    initialized: bool,
}

impl<'data> UdpSendOp<'data> {
    pub fn new(handle: Handle, addr: SocketAddrV4, data: &'data [u8]) -> Self {
        Self {
            handle,
            addr,
            data,
            initialized: false,
        }
    }
}

impl<X: Xfer> OpImpl<X> for UdpSendOp<'_> {
    type Output = ();
    type Error = StackError;

    fn poll_impl(
        &mut self,
        manager: &mut crate::manager::Manager<X>,
        callbacks: &mut SocketCallbacks,
    ) -> Result<Option<Self::Output>, Self::Error> {
        // Initialize UDP send request if not done yet
        if !self.initialized {
            let to_send = self.data.len().min(MAX_SEND_LENGTH);
            let req = SendRequest {
                offset: 0,
                grand_total_sent: 0,
                total_sent: 0,
                remaining: to_send as i16,
            };

            // Get the socket from the handle
            if let Some((sock, op)) = callbacks.udp_sockets.get(self.handle) {
                let socket = *sock; // Copy the socket

                // Send initial chunk
                manager
                    .send_sendto(socket, self.addr, &self.data[..to_send])
                    .map_err(StackError::SendSendFailed)?;

                *op =
                    ClientSocketOp::AsyncOp(AsyncOp::SendTo(req, None), AsyncState::Pending(None));

                self.initialized = true;
                return Ok(None); // Operation started, still in progress
            } else {
                return Err(StackError::SocketNotFound);
            }
        }

        // Check if send operation is complete
        if let Some((sock, op)) = callbacks.udp_sockets.get(self.handle) {
            let socket = *sock; // Copy the socket

            match op {
                ClientSocketOp::AsyncOp(AsyncOp::SendTo(req, Some(_len)), AsyncState::Done) => {
                    let total_sent = req.total_sent;
                    let grand_total_sent = req.grand_total_sent + total_sent;
                    let offset = req.offset + total_sent as usize;

                    if offset >= self.data.len() {
                        // Complete - reset operation
                        *op = ClientSocketOp::None;
                        return Ok(Some(()));
                    } else {
                        // Continue sending next chunk
                        let to_send = self.data[offset..].len().min(MAX_SEND_LENGTH);
                        let new_req = SendRequest {
                            offset,
                            grand_total_sent,
                            total_sent: 0,
                            remaining: to_send as i16,
                        };

                        *op = ClientSocketOp::AsyncOp(
                            AsyncOp::SendTo(new_req, None),
                            AsyncState::Pending(None),
                        );

                        manager
                            .send_sendto(socket, self.addr, &self.data[offset..offset + to_send])
                            .map_err(StackError::SendSendFailed)?;

                        return Ok(None); // Still in progress
                    }
                }
                ClientSocketOp::AsyncOp(AsyncOp::SendTo(_req, None), AsyncState::Pending(_)) => {
                    // Still waiting for callback response
                    return Ok(None);
                }
                _ => {
                    // No relevant operation in progress
                    return Ok(None);
                }
            }
        }

        // Still waiting for completion
        Ok(None)
    }
}
