use embedded_nal::TcpClientStack;
use embedded_nal::TcpFullStack;

use super::ClientSocketOp;
use super::Handle;
use super::StackError;
use super::WincClient;

use super::Xfer;
use crate::client::GenResult;
use crate::manager::SocketError;
use crate::Ipv4AddrFormatWrapper;
use crate::{debug, error};
use embedded_nal::nb;

impl<X: Xfer> WincClient<'_, X> {
    /// Todo: actually implement this
    pub fn set_socket_option(
        &mut self,
        socket: &Handle,
        option: u8,
        value: u32,
    ) -> Result<(), StackError> {
        let (sock, _op) = self.callbacks.tcp_sockets.get(*socket).unwrap();
        self.manager
            .send_setsockopt(*sock, option, value)
            .map_err(StackError::WincWifiFail)?;
        Ok(())
    }
}

impl<X: Xfer> embedded_nal::TcpClientStack for WincClient<'_, X> {
    type TcpSocket = Handle;
    type Error = StackError;
    fn socket(
        &mut self,
    ) -> Result<<Self as TcpClientStack>::TcpSocket, <Self as TcpClientStack>::Error> {
        self.dispatch_events()?;
        let s = self.get_next_session_id();
        let handle = self
            .callbacks
            .tcp_sockets
            .add(s)
            .ok_or(StackError::OutOfSockets)?;
        Ok(handle)
    }
    fn connect(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        remote: core::net::SocketAddr,
    ) -> Result<(), nb::Error<<Self as TcpClientStack>::Error>> {
        self.dispatch_events()?;
        match remote {
            core::net::SocketAddr::V4(addr) => {
                let (sock, op) = self.callbacks.tcp_sockets.get(*socket).unwrap();
                *op = ClientSocketOp::Connect;
                let op = *op;
                debug!("<> Sending send_socket_connect to {:?}", sock);
                self.manager
                    .send_socket_connect(*sock, addr)
                    .map_err(StackError::ConnectSendFailed)?;
                self.wait_for_op_ack(*socket, op, Self::CONNECT_TIMEOUT, true)?;
            }
            core::net::SocketAddr::V6(_) => unimplemented!("IPv6 not supported"),
        }
        Ok(())
    }
    fn send(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        data: &[u8],
    ) -> Result<usize, nb::Error<<Self as TcpClientStack>::Error>> {
        self.dispatch_events()?;

        // Send in chunks of up to 1400 bytes
        let mut offset = 0;
        while offset < data.len() {
            let to_send = data[offset..].len().min(Self::MAX_SEND_LENGTH);
            let (sock, op) = self.callbacks.tcp_sockets.get(*socket).unwrap();
            *op = ClientSocketOp::Send(to_send as i16);

            let op = *op;
            debug!("<> Sending socket send_send to {:?} len:{}", sock, to_send);
            self.manager
                .send_send(*sock, &data[offset..offset + to_send])
                .map_err(StackError::SendSendFailed)?;
            self.wait_for_op_ack(*socket, op, Self::SEND_TIMEOUT, true)?;
            offset += to_send;
        }
        Ok(data.len())
    }

    // Nb:: Blocking call, returns nb::Result when no data
    fn receive(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        data: &mut [u8],
    ) -> Result<usize, nb::Error<<Self as TcpClientStack>::Error>> {
        debug!("Receiving on socket {:?}", socket);
        self.dispatch_events()?;
        debug!("Receiving on socket {:?}", socket);
        let (sock, op) = self.callbacks.tcp_sockets.get(*socket).unwrap();
        *op = ClientSocketOp::Recv;
        let op = *op;
        let timeout = Self::RECV_TIMEOUT;
        debug!("<> Sending socket send_recv to {:?}", sock);
        self.manager
            .send_recv(*sock, timeout)
            .map_err(|x| nb::Error::Other(StackError::ReceiveFailed(x)))?;
        if let GenResult::Len(recv_len) =
            match self.wait_for_op_ack(*socket, op, self.recv_timeout, true) {
                Ok(result) => result,
                Err(StackError::OpFailed(SocketError::Timeout)) => {
                    return Err(nb::Error::WouldBlock)
                }
                Err(e) => return Err(nb::Error::Other(e)),
            }
        {
            let dest_slice = &mut data[..recv_len];
            dest_slice.copy_from_slice(&self.callbacks.recv_buffer[..recv_len]);
            Ok(recv_len)
        } else {
            Err(nb::Error::Other(StackError::Unexpected))
        }
    }
    fn close(&mut self, socket: <Self as TcpClientStack>::TcpSocket) -> Result<(), Self::Error> {
        debug!("Closing socket {:?}", socket);
        self.dispatch_events()?;
        let (sock, _op) = self.callbacks.tcp_sockets.get(socket).unwrap();
        self.manager
            .send_close(*sock)
            .map_err(StackError::SendCloseFailed)?;
        self.callbacks
            .tcp_sockets
            .get(socket)
            .ok_or(StackError::CloseFailed)?;
        self.callbacks.tcp_sockets.remove(socket);
        Ok(())
    }
}

impl<X: Xfer> TcpFullStack for WincClient<'_, X> {
    fn bind(&mut self, socket: &mut Self::TcpSocket, local_port: u16) -> Result<(), Self::Error> {
        self.dispatch_events()?;
        let (sock, op) = self.callbacks.tcp_sockets.get(*socket).unwrap();
        *op = ClientSocketOp::Bind;
        let op = *op;
        debug!("<> Sending socket bind to {:?}", sock);

        let server_addr =
            core::net::SocketAddrV4::new(core::net::Ipv4Addr::new(0, 0, 0, 0), local_port);

        self.manager
            .send_bind(*sock, server_addr)
            .map_err(StackError::BindFailed)?;
        self.wait_for_op_ack(*socket, op, Self::BIND_TIMEOUT, true)?;
        Ok(())
    }

    fn listen(&mut self, socket: &mut Self::TcpSocket) -> Result<(), Self::Error> {
        self.dispatch_events()?;
        let (sock, op) = self.callbacks.tcp_sockets.get(*socket).unwrap();
        *op = ClientSocketOp::Listen;
        let op = *op;
        debug!("<> Sending socket listen to {:?}", sock);
        self.manager.send_listen(*sock, Self::TCP_SOCKET_BACKLOG)?;
        self.wait_for_op_ack(*socket, op, Self::LISTEN_TIMEOUT, true)?;
        Ok(())
    }

    // This is a blocking call, return WouldBlock if no connection has been accepted
    fn accept(
        &mut self,
        socket: &mut Self::TcpSocket,
    ) -> nb::Result<(Self::TcpSocket, core::net::SocketAddr), Self::Error> {
        debug!("<> accept called on socket {:?}", socket);
        self.dispatch_events()?;
        let (sock, op) = self.callbacks.tcp_sockets.get(*socket).unwrap();
        debug!("<> Waiting for accept to socket {:?}", sock);
        *op = ClientSocketOp::Accept;
        let op = *op;
        // this needs to catch Err(StackError::GeneralTimeout) and map it to WouldBlock
        let res = match self.wait_for_op_ack(*socket, op, Self::ACCEPT_TIMEOUT, true) {
            Ok(res) => res,
            Err(StackError::GeneralTimeout) => return Err(nb::Error::WouldBlock),
            Err(e) => return Err(nb::Error::Other(e)),
        };
        match res {
            GenResult::Accept(addr, accepted_socket) => {
                debug!(
                    "Accept result: socket {:?} addr {:?} port {}",
                    accepted_socket,
                    Ipv4AddrFormatWrapper::new(addr.ip()),
                    addr.port(),
                );
                let handle = self
                    .callbacks
                    .tcp_sockets
                    .put(Handle(accepted_socket.v), accepted_socket.s)
                    .ok_or(StackError::SocketAlreadyInUse)?;
                Ok((handle, core::net::SocketAddr::V4(addr)))
            }
            _ => {
                error!("<> Accept failed, we got unexpected");
                Err(nb::Error::Other(StackError::Unexpected))
            }
        }
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::client::test_shared::*;
    use crate::{client::SocketCallbacks, manager::EventListener, socket::Socket};
    use core::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
    use embedded_nal::{TcpClientStack, TcpFullStack};

    #[test]
    fn test_tcp_socket_open() {
        let mut client = make_test_client();
        let tcp_socket = client.socket();
        assert!(tcp_socket.is_ok());
    }

    #[test]
    fn test_tcp_connect() {
        let mut client = make_test_client();
        let mut tcp_socket = client.socket().unwrap();

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connect(Socket::new(0, 0), SocketError::NoError);
        };

        client.debug_callback = Some(&mut my_debug);
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 80);

        let result = nb::block!(client.connect(&mut tcp_socket, socket_addr));

        assert!(result.is_ok());
    }

    #[test]
    fn test_tcp_connect_check_blocking() {
        let mut client = make_test_client();
        let mut tcp_socket = client.socket().unwrap();
        let mut counter: u8 = 0;

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connect(Socket::new(0, 0), SocketError::NoError);
        };

        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 80);

        while counter != 5 {
            let result = nb::block!(client.connect(&mut tcp_socket, socket_addr));
            assert!(result.is_err());
            counter += 1;
        }

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.connect(&mut tcp_socket, socket_addr));
        assert!(result.is_ok());
    }

    #[test]
    fn test_tcp_send() {
        let mut client = make_test_client();
        let mut tcp_socket = client.socket().unwrap();
        let packet = "Hello, World";

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_send(Socket::new(0, 0), packet.len() as i16);
        };

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.send(&mut tcp_socket, packet.as_bytes()));

        assert_eq!(result.ok(), Some(packet.len()));
    }

    #[test]
    fn test_tcp_receive() {
        let mut client = make_test_client();
        let mut tcp_socket = client.socket().unwrap();
        let socket_addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 80);
        let mut recv_buff = [0u8; 32];
        let test_data = "Hello, World";

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_recv(
                Socket::new(0, 0),
                socket_addr,
                test_data.as_bytes(),
                SocketError::NoError,
            );
        };

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.receive(&mut tcp_socket, &mut recv_buff));

        assert_eq!(result.ok(), Some(test_data.len()));
    }

    #[test]
    fn test_tcp_close() {
        let mut client = make_test_client();
        let tcp_socket = client.socket().unwrap();

        let result = client.close(tcp_socket);

        assert!(result.is_ok());
    }

    #[test]
    fn test_tcp_bind() {
        let mut client = make_test_client();
        let mut tcp_socket = client.socket().unwrap();

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_bind(Socket::new(0, 0), SocketError::NoError);
        };

        client.debug_callback = Some(&mut my_debug);

        let result = client.bind(&mut tcp_socket, 8080);

        assert!(result.is_ok());
    }

    #[test]
    fn test_tcp_listen() {
        let mut client = make_test_client();
        let mut tcp_socket = client.socket().unwrap();

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_listen(Socket::new(0, 0), SocketError::NoError);
        };

        client.debug_callback = Some(&mut my_debug);

        let result = client.listen(&mut tcp_socket);

        assert!(result.is_ok());
    }

    #[test]
    fn test_tcp_accept() {
        let mut client = make_test_client();
        let mut tcp_socket = client.socket().unwrap();
        let socket_addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 80);

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_accept(socket_addr, Socket::new(0, 0), Socket::new(1, 0), 0);
        };

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.accept(&mut tcp_socket));

        assert!(result.is_ok());
    }
}
