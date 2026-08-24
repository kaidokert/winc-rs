use crate::manager::SocketError;
use crate::ops::op::OpImpl;
use crate::stack::sock_holder::SocketStore;
use crate::stack::socket_callbacks::SocketCallbacks;
use crate::stack::socket_callbacks::TcpRecvState;
use crate::transfer::Xfer;
use crate::Handle;
use crate::StackError;

#[derive(Debug)]
pub struct TcpReceiveOp<'buffer> {
    handle: Handle,
    buffer: &'buffer mut [u8],
}

impl<'buffer> TcpReceiveOp<'buffer> {
    pub fn new(handle: Handle, buffer: &'buffer mut [u8]) -> Self {
        Self { handle, buffer }
    }
}

impl<X: Xfer> OpImpl<X> for TcpReceiveOp<'_> {
    type Output = usize;
    type Error = StackError;

    fn poll_impl(
        &mut self,
        manager: &mut crate::manager::Manager<X>,
        callbacks: &mut SocketCallbacks,
    ) -> Result<Option<Self::Output>, Self::Error> {
        let (sock, _) = callbacks
            .tcp_sockets
            .get(self.handle)
            .ok_or(StackError::SocketNotFound)?;
        let socket = *sock;
        let index = socket.v as usize;

        // Receive state lives per socket, not in the socket's single op slot:
        // a send in flight must not disturb an outstanding receive.
        match callbacks.tcp_recv[index] {
            TcpRecvState::Ready(mut result) => {
                match result.error {
                    SocketError::NoError => {}
                    SocketError::Timeout => {
                        // Timeouts are retried rather than surfaced.
                        manager
                            .send_recv(socket, socket.get_recv_timeout())
                            .map_err(StackError::ReceiveFailed)?;
                        callbacks.tcp_recv[index] = TcpRecvState::Requested;
                        return Ok(None);
                    }
                    error => {
                        callbacks.tcp_recv[index] = TcpRecvState::Idle;
                        return Err(StackError::OpFailed(error));
                    }
                }
                if result.recv_len == 0 {
                    callbacks.tcp_recv[index] = TcpRecvState::Idle;
                    return Ok(Some(0));
                }
                let remaining = result.recv_len - result.return_offset;
                let copy_len = remaining.min(self.buffer.len());
                let from = result.return_offset;
                self.buffer[..copy_len]
                    .copy_from_slice(&callbacks.recv_buffer[from..from + copy_len]);
                result.return_offset += copy_len;
                callbacks.tcp_recv[index] = if result.return_offset >= result.recv_len {
                    TcpRecvState::Idle
                } else {
                    TcpRecvState::Ready(result)
                };
                Ok(Some(copy_len))
            }
            TcpRecvState::Requested => Ok(None),
            TcpRecvState::Idle => {
                manager
                    .send_recv(socket, socket.get_recv_timeout())
                    .map_err(StackError::ReceiveFailed)?;
                callbacks.tcp_recv[index] = TcpRecvState::Requested;
                Ok(None)
            }
        }
    }
}
