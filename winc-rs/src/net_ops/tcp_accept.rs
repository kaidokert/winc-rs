use core::net::SocketAddrV4;

use super::op::OpImpl;
use crate::client::ClientSocketOp;
use crate::socket::Socket;
use crate::stack::sock_holder::SocketStore;
use crate::stack::socket_callbacks::SocketCallbacks;
use crate::stack::socket_callbacks::{AsyncOp, AsyncState};
use crate::transfer::Xfer;
use crate::Handle;
use crate::StackError;

#[derive(Debug)]
pub struct TcpAcceptOp {
    handle: Handle,
}

impl TcpAcceptOp {
    pub fn new(handle: Handle) -> Self {
        Self { handle }
    }
}

impl<X: Xfer> OpImpl<X> for TcpAcceptOp {
    type Output = (Socket, SocketAddrV4);
    type Error = StackError;

    fn poll_impl(
        &mut self,
        _manager: &mut crate::manager::Manager<X>,
        callbacks: &mut SocketCallbacks,
    ) -> Result<Option<Self::Output>, Self::Error> {
        let (_sock, op) = callbacks
            .tcp_sockets
            .get(self.handle)
            .ok_or(StackError::SocketNotFound)?;

        match op {
            ClientSocketOp::AsyncOp(AsyncOp::Accept(Some(accept_result)), _) => {
                let accepted_socket = accept_result.accepted_socket;
                let addr = accept_result.accept_addr;
                *op = ClientSocketOp::None;
                Ok(Some((accepted_socket, addr)))
            }
            ClientSocketOp::AsyncOp(AsyncOp::Accept(None), AsyncState::Pending(_)) => Ok(None),
            _ => {
                // No manager command to send - just set up the pending state
                *op = ClientSocketOp::AsyncOp(AsyncOp::Accept(None), AsyncState::Pending(None));
                Ok(None)
            }
        }
    }
}
