use crate::client::ClientSocketOp;
use crate::manager::SocketError;
use crate::ops::op::OpImpl;
use crate::stack::sock_holder::SocketStore;
use crate::stack::socket_callbacks::SocketCallbacks;
use crate::stack::socket_callbacks::{AsyncOp, AsyncState};
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
        let (sock, op) = callbacks
            .tcp_sockets
            .get(self.handle)
            .ok_or(StackError::SocketNotFound)?;
        let socket = *sock;

        // A delivery parked by `on_recv` because the op slot was busy with a
        // send. Drained before the op state machine, and before the leftover
        // path, because it is strictly older than anything a subsequent recv
        // request will produce -- handing it back out of order would reorder
        // the byte stream, which is the defect this exists to prevent.
        if let Some((psock, ref mut pending)) = callbacks.pending_recv {
            if psock.v == socket.v {
                if pending.return_offset < pending.recv_len {
                    let remaining = pending.recv_len - pending.return_offset;
                    let copy_len = remaining.min(self.buffer.len());
                    let from = pending.return_offset;
                    self.buffer[..copy_len]
                        .copy_from_slice(&callbacks.recv_buffer[from..from + copy_len]);
                    pending.return_offset += copy_len;
                    let done = pending.return_offset >= pending.recv_len;
                    if done {
                        callbacks.pending_recv = None;
                    }
                    return Ok(Some(copy_len));
                }
                callbacks.pending_recv = None;
            }
        }

        // First, handle leftover data from a previous operation
        if let ClientSocketOp::AsyncOp(AsyncOp::Recv(Some(ref mut recv_result)), AsyncState::Done) =
            op
        {
            if recv_result.return_offset < recv_result.recv_len
                && recv_result.error == SocketError::NoError
            {
                let remaining_data = recv_result.recv_len - recv_result.return_offset;
                let copy_len = remaining_data.min(self.buffer.len());

                self.buffer[..copy_len].copy_from_slice(
                    &callbacks.recv_buffer
                        [recv_result.return_offset..recv_result.return_offset + copy_len],
                );
                recv_result.return_offset += copy_len;

                if recv_result.return_offset >= recv_result.recv_len {
                    *op = ClientSocketOp::None;
                }
                return Ok(Some(copy_len));
            }
        }

        // If no leftover data, proceed with the operation state machine
        match op {
            ClientSocketOp::AsyncOp(AsyncOp::Recv(Some(recv_result)), _) => {
                match recv_result.error {
                    SocketError::NoError => {
                        let recv_len = recv_result.recv_len;
                        if recv_len == 0 {
                            *op = ClientSocketOp::None;
                            return Ok(Some(0));
                        }
                        let copy_len = recv_len.min(self.buffer.len());
                        self.buffer[..copy_len].copy_from_slice(&callbacks.recv_buffer[..copy_len]);
                        recv_result.return_offset = copy_len;

                        if copy_len >= recv_len {
                            *op = ClientSocketOp::None;
                        }
                        Ok(Some(copy_len))
                    }
                    SocketError::Timeout => {
                        manager
                            .send_recv(socket, socket.get_recv_timeout())
                            .map_err(StackError::ReceiveFailed)?;
                        *op =
                            ClientSocketOp::AsyncOp(AsyncOp::Recv(None), AsyncState::Pending(None));
                        Ok(None)
                    }
                    error => {
                        *op = ClientSocketOp::None;
                        Err(StackError::OpFailed(error))
                    }
                }
            }
            ClientSocketOp::AsyncOp(AsyncOp::Recv(None), AsyncState::Pending(_)) => Ok(None),
            _ => {
                manager
                    .send_recv(socket, socket.get_recv_timeout())
                    .map_err(StackError::ReceiveFailed)?;
                *op = ClientSocketOp::AsyncOp(AsyncOp::Recv(None), AsyncState::Pending(None));
                Ok(None)
            }
        }
    }
}
