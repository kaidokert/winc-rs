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
            ClientSocketOp::AsyncOp(AsyncOp::Send(req, Some(_len)), AsyncState::Done) => {
                let total_sent = req.total_sent;
                let grand_total_sent = req.grand_total_sent + total_sent;
                let offset = req.offset + total_sent as usize;

                if offset >= self.data.len() {
                    // Complete - reset operation
                    *op = ClientSocketOp::None;
                    Ok(Some(grand_total_sent as usize))
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
                        AsyncOp::Send(new_req, None),
                        AsyncState::Pending(None),
                    );
                    manager
                        .send_send(socket, &self.data[offset..offset + to_send])
                        .map_err(StackError::SendSendFailed)?;
                    Ok(None) // Still in progress
                }
            }
            ClientSocketOp::AsyncOp(AsyncOp::Send(_, None), AsyncState::Pending(_)) => {
                // Still waiting for callback response
                Ok(None)
            }
            _ => {
                // Not started or in an unexpected state, so initialize
                let to_send = self.data.len().min(MAX_SEND_LENGTH);
                let req = SendRequest {
                    offset: 0,
                    grand_total_sent: 0,
                    total_sent: 0,
                    remaining: to_send as i16,
                };
                manager
                    .send_send(socket, &self.data[..to_send])
                    .map_err(StackError::SendSendFailed)?;
                *op = ClientSocketOp::AsyncOp(AsyncOp::Send(req, None), AsyncState::Pending(None));
                Ok(None)
            }
        }
    }
}
