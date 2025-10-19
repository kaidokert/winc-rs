use super::op::OpImpl;
use crate::client::ClientSocketOp;
use crate::stack::constants::MAX_SEND_LENGTH;
use crate::stack::sock_holder::SocketStore;
use crate::stack::socket_callbacks::SocketCallbacks;
use crate::stack::socket_callbacks::{AsyncOp, AsyncState, SendRequest};
use crate::transfer::Xfer;
use crate::Handle;
use crate::StackError;

#[derive(Debug)]
pub struct TcpSendOp<'data> {
    handle: Handle,
    data: &'data [u8],
}

impl<'data> TcpSendOp<'data> {
    pub fn new(handle: Handle, data: &'data [u8]) -> Self {
        Self { handle, data }
    }
}

impl<X: Xfer> OpImpl<X> for TcpSendOp<'_> {
    type Output = usize;
    type Error = StackError;

    fn poll_impl(
        &mut self,
        manager: &mut crate::manager::Manager<X>,
        callbacks: &mut SocketCallbacks,
    ) -> Result<Option<Self::Output>, Self::Error> {
        let (sock, op) = callbacks
            .tcp_sockets
            .get(self.handle)
            .ok_or(StackError::SocketNotFound)?;
        let socket = *sock;

        match op {
            ClientSocketOp::AsyncOp(AsyncOp::Send(req, Some(len)), AsyncState::Done) => {
                // Validate callback length parameter
                let callback_len = *len;
                if callback_len < 0 {
                    // Negative length is invalid - treat as error
                    *op = ClientSocketOp::None;
                    return Err(StackError::InvalidParameters);
                }

                // Validate callback length doesn't exceed remaining data in buffer
                let remaining_in_buffer = self.data.len() - req.offset;
                let validated_len = if callback_len as usize > remaining_in_buffer {
                    // Clamp to remaining data if callback reports more than possible
                    remaining_in_buffer
                } else {
                    callback_len as usize
                };

                // Ensure validated_len fits in i16 for arithmetic
                if validated_len > i16::MAX as usize {
                    *op = ClientSocketOp::None;
                    return Err(StackError::InvalidParameters);
                }

                let validated_len_i16 = validated_len as i16;

                // Compute new offset using safe checked arithmetic
                let offset = req
                    .offset
                    .checked_add(validated_len)
                    .ok_or(StackError::InvalidParameters)?;

                // Update grand_total_sent using validated length
                let grand_total_sent = req
                    .grand_total_sent
                    .checked_add(validated_len_i16)
                    .ok_or(StackError::InvalidParameters)?;

                if offset >= self.data.len() {
                    // Complete - reset operation
                    *op = ClientSocketOp::None;
                    Ok(Some(usize::from(grand_total_sent as u16)))
                } else {
                    // Continue sending next chunk
                    let remaining_data = self.data.len() - offset;
                    let to_send = remaining_data.min(MAX_SEND_LENGTH);

                    // Ensure to_send fits in i16
                    if to_send > i16::MAX as usize {
                        *op = ClientSocketOp::None;
                        return Err(StackError::InvalidParameters);
                    }

                    let new_req = SendRequest {
                        offset,
                        grand_total_sent,
                        total_sent: 0,
                        remaining: to_send as i16,
                    };

                    // Set operation state BEFORE calling send_send to avoid reentrancy races
                    let prev_op = core::mem::replace(
                        op,
                        ClientSocketOp::AsyncOp(
                            AsyncOp::Send(new_req, None),
                            AsyncState::Pending(None),
                        ),
                    );

                    // Now call send_send - if it fails, revert to previous state
                    match manager.send_send(socket, &self.data[offset..offset + to_send]) {
                        Ok(()) => Ok(None), // Still in progress
                        Err(e) => {
                            // Revert to previous state on failure
                            *op = prev_op;
                            Err(StackError::SendSendFailed(e))
                        }
                    }
                }
            }
            ClientSocketOp::AsyncOp(AsyncOp::Send(_, None), AsyncState::Pending(_)) => {
                // Still waiting for callback response
                Ok(None)
            }
            _ => {
                // Not started or in an unexpected state, so initialize
                let to_send = self.data.len().min(MAX_SEND_LENGTH);

                // Ensure to_send fits in i16
                if to_send > i16::MAX as usize {
                    return Err(StackError::InvalidParameters);
                }

                let req = SendRequest {
                    offset: 0,
                    grand_total_sent: 0,
                    total_sent: 0,
                    remaining: to_send as i16,
                };

                // Set operation state BEFORE calling send_send to avoid reentrancy races
                // if callbacks run synchronously
                let prev_op = core::mem::replace(
                    op,
                    ClientSocketOp::AsyncOp(AsyncOp::Send(req, None), AsyncState::Pending(None)),
                );

                // Now call send_send - if it fails, revert to previous state
                match manager.send_send(socket, &self.data[..to_send]) {
                    Ok(()) => Ok(None), // Still in progress
                    Err(e) => {
                        // Revert to previous state on failure
                        *op = prev_op;
                        Err(StackError::SendSendFailed(e))
                    }
                }
            }
        }
    }
}
