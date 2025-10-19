use core::net::SocketAddrV4;

use super::op::OpImpl;
use crate::client::ClientSocketOp;
use crate::manager::SocketError;
use crate::stack::sock_holder::SocketStore;
use crate::stack::socket_callbacks::SocketCallbacks;
use crate::stack::socket_callbacks::{AsyncOp, AsyncState, ConnectResult};
use crate::transfer::Xfer;
use crate::Handle;
use crate::StackError;

#[derive(Debug)]
pub struct TcpConnectOp {
    handle: Handle,
    addr: SocketAddrV4,
}

impl TcpConnectOp {
    pub fn new(handle: Handle, addr: SocketAddrV4) -> Self {
        Self { handle, addr }
    }
}

impl<X: Xfer> OpImpl<X> for TcpConnectOp {
    type Output = ();
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
            ClientSocketOp::AsyncOp(AsyncOp::Connect(Some(ConnectResult { error })), _) => {
                let error = *error;
                *op = ClientSocketOp::None;
                match error {
                    SocketError::NoError => Ok(Some(())),
                    _ => Err(StackError::OpFailed(error)),
                }
            }
            ClientSocketOp::AsyncOp(AsyncOp::Connect(None), AsyncState::Pending(_)) => Ok(None),
            _ => {
                // Set operation state BEFORE calling send_socket_connect to avoid reentrancy races
                let prev_op = core::mem::replace(
                    op,
                    ClientSocketOp::AsyncOp(AsyncOp::Connect(None), AsyncState::Pending(None)),
                );

                // Now call send_socket_connect - if it fails, revert to previous state
                match manager.send_socket_connect(socket, self.addr) {
                    Ok(()) => Ok(None),
                    Err(e) => {
                        // Revert to previous state on failure
                        *op = prev_op;
                        Err(StackError::ConnectSendFailed(e))
                    }
                }
            }
        }
    }
}
