use super::AsyncClient;
use crate::net_ops::{tcp_connect::TcpConnectOp, tcp_receive::TcpReceiveOp, tcp_send::TcpSendOp};
use crate::stack::{sock_holder::SocketStore, socket_callbacks::Handle};
use crate::transfer::Xfer;
use crate::StackError;
use embedded_nal_async::TcpConnect;

/// Structure for Asynchronous TCP connection.
pub struct AsyncTcpConnection<'a, 'b, X: Xfer> {
    client: &'b AsyncClient<'a, X>,
    socket: Option<Handle>,
}

/// Implements `embedded-io-async::ErrorType` for TCP connection errors.
impl<'a, 'b, X: Xfer> embedded_io_async::ErrorType for AsyncTcpConnection<'a, 'b, X> {
    type Error = StackError;
}

/// Implements `embedded_io_async::Read` for receiving data from the TCP connection.
impl<'a, 'b, X: Xfer> embedded_io_async::Read for AsyncTcpConnection<'a, 'b, X> {
    /// Reads data from the TCP connection asynchronously.
    ///
    /// # Arguments
    ///
    /// * `buf` - A mutable byte slice where the received data will be stored.
    ///
    /// # Returns
    ///
    /// * `Ok(usize)` - The number of bytes successfully read into `buf`.
    /// * `Err(Self::Error)` - If an error occurs during the read operation.
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

/// Implements `embedded_io_async::Write` for writing data to the TCP connection.
impl<'a, 'b, X: Xfer> embedded_io_async::Write for AsyncTcpConnection<'a, 'b, X> {
    /// Sends data to the TCP connection asynchronously.
    ///
    /// # Arguments
    ///
    /// * `buf` - A byte slice containing the data to be sent to the TCP connection.
    ///
    /// # Returns
    ///
    /// * `Ok(usize)` - The number of bytes successfully sent.
    /// * `Err(Self::Error)` - If an error occurs during the write operation.
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        // No bytes to send.
        if buf.is_empty() {
            return Ok(0);
        }

        let socket = self.socket.ok_or(StackError::SocketNotFound)?;
        let mut op = TcpSendOp::new(socket, buf);

        self.client.poll_op(&mut op).await
    }

    /// Send all data buffered for sending to the TCP connection.
    async fn flush(&mut self) -> Result<(), Self::Error> {
        // The `write` operation polls until all data is passed to the manager,
        // so there is no internal buffering to flush at this layer.
        Ok(())
    }
}

impl<'a, X: Xfer> TcpConnect for AsyncClient<'a, X> {
    /// Error type returned on connect failure.
    type Error = StackError;

    /// Type holding state of a TCP connection.
    type Connection<'b>
        = AsyncTcpConnection<'a, 'b, X>
    where
        Self: 'b;

    /// Establishes a TCP connection to a remote socket address.
    ///
    /// # Arguments
    ///
    /// * `remote` - The remote socket address (`core::net::SocketAddr`) to connect to.
    ///
    /// # Returns
    ///
    /// * `Ok(Self::Connection<'_>)` - A newly established TCP connection.
    /// * `Err(Self::Error)` - If the connection attempt fails.
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

        if let Err(e) = self.poll_op(&mut tcp_connect_op).await {
            self.close_tcp_handle(handle);
            return Err(e);
        }

        Ok(AsyncTcpConnection {
            client: self,
            socket: Some(handle),
        })
    }
}

/// Implements cleanup for `AsyncTcpConnection` when it goes out of scope.
impl<'a, 'b, X: Xfer> Drop for AsyncTcpConnection<'a, 'b, X> {
    /// This `Drop` implementation ensures that any TCP socket associated with the
    /// connection is properly closed when the `AsyncTcpConnection` instance is dropped.
    fn drop(&mut self) {
        if let Some(socket) = self.socket.take() {
            self.client.close_tcp_handle(socket);
        }
    }
}

impl<X: Xfer> AsyncClient<'_, X> {
    /// Allocates a new TCP socket.
    ///
    /// # Returns
    ///
    /// * `Ok(Handle)` - A handle to the newly allocated TCP socket.
    /// * `Err(StackError)` - If the socket could not be allocated.
    pub(crate) fn allocate_tcp_sockets(&self) -> Result<Handle, StackError> {
        let session_id = self.get_next_session_id();
        let mut callbacks = self.callbacks.borrow_mut();
        callbacks
            .tcp_sockets
            .add(session_id)
            .ok_or(StackError::OutOfSockets)
    }

    /// Closes the provided TCP socket.
    ///
    /// # Arguments
    ///
    /// * `handle` - The `Handle` of the TCP socket to be closed.
    pub(crate) fn close_tcp_handle(&self, handle: Handle) {
        // Use try_borrow_mut to avoid panicking in Drop if already borrowed
        if let (Ok(mut manager), Ok(mut callbacks)) = (
            self.manager.try_borrow_mut(),
            self.callbacks.try_borrow_mut(),
        ) {
            if let Some((sock, _)) = callbacks.tcp_sockets.get(handle) {
                if let Err(e) = manager.send_close(*sock) {
                    crate::error!("Failed to close TCP socket {:?} in drop: {:?}", sock, e);
                } else {
                    callbacks.tcp_sockets.remove(handle);
                }
            }
        } else {
            crate::error!(
                "Failed to clean up TCP socket handle {:?}: resources busy",
                handle
            );
        }
    }
}

#[cfg(test)]
mod test {
    use super::super::tests::make_test_client;
    use super::*;
    use crate::manager::{EventListener, SocketError};
    use crate::socket::Socket;
    use crate::stack::socket_callbacks::SocketCallbacks;
    use core::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4};
    use embedded_io_async::{Read, Write};
    use macro_rules_attribute::apply;
    use smol_macros::test;

    #[apply(test!)]
    async fn test_async_tcp_connect_success() {
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 80);

        let mut debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connect(Socket::new(0, 0), SocketError::NoError);
        };

        let client = make_test_client();

        let result = {
            *client.debug_callback.borrow_mut() = Some(&mut debug);
            client.connect(socket_addr).await
        };

        assert!(result.is_ok());
    }

    #[apply(test!)]
    async fn test_async_tcp_connect_fail() {
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 80);

        let mut debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connect(Socket::new(0, 0), SocketError::InvalidAddress);
        };

        let client = make_test_client();

        let result = {
            *client.debug_callback.borrow_mut() = Some(&mut debug);
            client.connect(socket_addr).await
        };

        assert_eq!(
            result.err(),
            Some(StackError::OpFailed(SocketError::InvalidAddress))
        );
    }

    #[apply(test!)]
    async fn test_async_tcp_connect_ipv6_fail() {
        let socket_addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)), 80);

        let client = make_test_client();

        let result = client.connect(socket_addr).await;

        assert_eq!(result.err(), Some(StackError::InvalidParameters));
    }

    #[apply(test!)]
    async fn test_async_tcp_connect_port_fail() {
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);

        let client = make_test_client();

        let result = client.connect(socket_addr).await;

        assert_eq!(result.err(), Some(StackError::InvalidParameters));
    }

    #[apply(test!)]
    async fn test_async_tcp_write_success() {
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 80);
        let socket = Socket::new(0, 0);

        let mut conn_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connect(socket, SocketError::NoError);
        };

        let mut send_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_send(socket, 32);
        };

        let client = make_test_client();

        let mut tcp_client = {
            // Set tcp connect callback
            *client.debug_callback.borrow_mut() = Some(&mut conn_debug);
            let tcp_client = client.connect(socket_addr).await;
            assert!(tcp_client.is_ok());

            tcp_client.unwrap()
        };

        // set tcp send callback
        *client.debug_callback.borrow_mut() = Some(&mut send_debug);

        let buffer = [1u8; 32];
        let result = tcp_client.write(&buffer).await;

        assert_eq!(result, Ok(32));
    }

    #[apply(test!)]
    async fn test_async_tcp_write_empty_buffer() {
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 80);

        let mut debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connect(Socket::new(0, 0), SocketError::NoError);
        };

        let client = make_test_client();

        let mut tcp_client = {
            *client.debug_callback.borrow_mut() = Some(&mut debug);
            let tcp_client = client.connect(socket_addr).await;
            assert!(tcp_client.is_ok());

            tcp_client.unwrap()
        };

        let result = tcp_client.write(&[]).await;

        assert_eq!(result, Ok(0));
    }

    #[apply(test!)]
    async fn test_async_tcp_write_fail() {
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 80);
        let socket = Socket::new(0, 0);

        let mut debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connect(socket, SocketError::NoError);
        };

        let mut send_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_send(socket, -12);
        };

        let client = make_test_client();

        let mut tcp_client = {
            // Set tcp connect callback
            *client.debug_callback.borrow_mut() = Some(&mut debug);
            let tcp_client = client.connect(socket_addr).await;
            assert!(tcp_client.is_ok());

            tcp_client.unwrap()
        };

        // set tcp send callback
        *client.debug_callback.borrow_mut() = Some(&mut send_debug);

        let buffer = [1u8; 32];
        let result = tcp_client.write(&buffer).await;

        assert_eq!(result, Err(StackError::OpFailed(SocketError::ConnAborted)));
    }

    #[apply(test!)]
    async fn test_async_tcp_read_success() {
        let remote_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 80);
        let remote_addr_v4 = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 80);
        let socket = Socket::new(0, 0);

        let mut debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connect(socket, SocketError::NoError);
        };

        let mut recv_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_recv(socket, remote_addr_v4, &[0xAA; 1400], SocketError::NoError);
        };

        let client = make_test_client();

        let mut tcp_client = {
            // Set tcp connect callback
            *client.debug_callback.borrow_mut() = Some(&mut debug);
            let tcp_client = client.connect(remote_addr).await;
            assert!(tcp_client.is_ok());

            tcp_client.unwrap()
        };

        // set tcp send callback
        *client.debug_callback.borrow_mut() = Some(&mut recv_debug);

        let mut buffer = [0u8; 1400];
        let result = tcp_client.read(&mut buffer).await;

        assert_eq!(result, Ok(1400));
        assert!(buffer.iter().all(|&x| x == 0xAA));
    }

    #[apply(test!)]
    async fn test_async_tcp_read_fail() {
        let remote_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 80);
        let remote_addr_v4 = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 80);
        let socket = Socket::new(0, 0);

        let mut debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connect(socket, SocketError::NoError);
        };

        let mut recv_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_recv(socket, remote_addr_v4, &[0], SocketError::Invalid);
        };

        let client = make_test_client();

        let mut tcp_client = {
            // Set tcp connect callback
            *client.debug_callback.borrow_mut() = Some(&mut debug);
            let tcp_client = client.connect(remote_addr).await;
            assert!(tcp_client.is_ok());

            tcp_client.unwrap()
        };

        // set tcp send callback
        *client.debug_callback.borrow_mut() = Some(&mut recv_debug);

        let mut buffer = [0u8; 1400];
        let result = tcp_client.read(&mut buffer).await;

        assert_eq!(result, Err(StackError::OpFailed(SocketError::Invalid)));
    }

    #[apply(test!)]
    async fn test_async_tcp_read_empty_buffer() {
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 80);

        let mut debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connect(Socket::new(0, 0), SocketError::NoError);
        };

        let client = make_test_client();

        let mut tcp_client = {
            *client.debug_callback.borrow_mut() = Some(&mut debug);
            let tcp_client = client.connect(socket_addr).await;
            assert!(tcp_client.is_ok());

            tcp_client.unwrap()
        };

        let result = tcp_client.read(&mut []).await;

        assert_eq!(result, Ok(0));
    }

    #[apply(test!)]
    async fn test_async_tcp_flush_success() {
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 80);
        let socket = Socket::new(0, 0);

        let mut conn_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connect(socket, SocketError::NoError);
        };

        let client = make_test_client();

        let mut tcp_client = {
            // Set tcp connect callback
            *client.debug_callback.borrow_mut() = Some(&mut conn_debug);
            let tcp_client = client.connect(socket_addr).await;
            assert!(tcp_client.is_ok());

            tcp_client.unwrap()
        };

        let result = tcp_client.flush().await;

        assert_eq!(result, Ok(()));
    }
}
