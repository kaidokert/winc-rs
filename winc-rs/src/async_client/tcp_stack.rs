use super::AsyncClient;
use crate::net_ops::{tcp_connect::TcpConnectOp, tcp_receive::TcpReceiveOp, tcp_send::TcpSendOp};
use crate::stack::{sock_holder::SocketStore, socket_callbacks::Handle};
use crate::transfer::Xfer;
use crate::CommError as Error;
use crate::StackError;
use embedded_nal_async::TcpConnect;

// TODO: Not sure this should be public
pub struct AsyncTcpConnection<'a, 'b, X: Xfer> {
    client: &'b AsyncClient<'a, X>,
    socket: Option<Handle>,
}

// Implement embedded-io-async traits for AsyncTcpConnection
impl<'a, 'b, X: Xfer> embedded_io_async::ErrorType for AsyncTcpConnection<'a, 'b, X> {
    type Error = StackError;
}

impl<'a, 'b, X: Xfer> embedded_io_async::Read for AsyncTcpConnection<'a, 'b, X> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        // No bytes to read.
        if buf.is_empty() {
            return Ok(0);
        }

        let socket = self.socket.ok_or(StackError::SocketNotFound)?;
        let mut op = TcpReceiveOp::new(socket, buf);
        self.client.poll_op(&mut op).await
    }
}

impl<'a, 'b, X: Xfer> embedded_io_async::Write for AsyncTcpConnection<'a, 'b, X> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        // No bytes to send.
        if buf.is_empty() {
            return Ok(0);
        }

        let socket = self.socket.ok_or(StackError::SocketNotFound)?;
        let mut op = TcpSendOp::new(socket, buf);
        let sent_buffer = self.client.poll_op(&mut op).await?;

        // If no data is sent return error.
        if sent_buffer == 0 && !buf.is_empty() {
            return Err(StackError::SendSendFailed(Error::WriteError));
        }

        Ok(sent_buffer)
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        todo!()
    }
}

impl<'a, X: Xfer> TcpConnect for AsyncClient<'a, X> {
    type Error = StackError;
    type Connection<'b>
        = AsyncTcpConnection<'a, 'b, X>
    where
        Self: 'b;

    async fn connect(
        &self,
        remote: core::net::SocketAddr,
    ) -> Result<Self::Connection<'_>, Self::Error> {
        let core::net::SocketAddr::V4(addr) = remote else {
            return Err(StackError::InvalidParameters);
        };

        // validate remote port
        if addr.port() == 0 {
            return Err(StackError::InvalidParameters);
        }

        // create new socket
        let handle = self.allocate_tcp_sockets()?;

        // New TCP socket
        let mut tcp_connect_op = TcpConnectOp::new(handle, addr);

        self.poll_op(&mut tcp_connect_op).await?;

        Ok(AsyncTcpConnection {
            client: self,
            socket: Some(handle),
        })
    }
}

impl<'a, 'b, X: Xfer> Drop for AsyncTcpConnection<'a, 'b, X> {
    fn drop(&mut self) {
        if let Some(socket) = self.socket.take() {
            self.client.close_tcp_handle(socket);
        }
    }
}

impl<X: Xfer> AsyncClient<'_, X> {
    pub(crate) fn allocate_tcp_sockets(&self) -> Result<Handle, StackError> {
        let session_id = self.get_next_session_id();
        let mut callbacks = self.callbacks.borrow_mut();
        callbacks
            .tcp_sockets
            .add(session_id)
            .ok_or(StackError::OutOfSockets)
    }

    pub(crate) fn close_tcp_handle(&self, handle: Handle) {
        // Use try_borrow_mut to avoid panicking in Drop if already borrowed
        if let (Ok(mut manager), Ok(mut callbacks)) = (
            self.manager.try_borrow_mut(),
            self.callbacks.try_borrow_mut(),
        ) {
            if let Some((sock, _)) = callbacks.tcp_sockets.get(handle) {
                if let Err(e) = manager.send_close(*sock) {
                    crate::warn!("Failed to close UDP socket {:?} in drop: {:?}", sock, e);
                }
                callbacks.tcp_sockets.remove(handle);
            }
        }
    }
}
