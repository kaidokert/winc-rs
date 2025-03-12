use embedded_nal::TcpClientStack;
use embedded_nal::TcpFullStack;

use super::ClientSocketOp;
use super::Handle;
use super::StackError;
use super::WincClient;

use super::Xfer;
use crate::debug;
use crate::manager::SocketError;
use crate::stack::socket_callbacks::SendRequest;
use embedded_nal::nb;

use crate::handle_result;

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
        match remote {
            core::net::SocketAddr::V4(addr) => {
                let (sock, op) = self.callbacks.tcp_sockets.get(*socket).unwrap();
                let res = match op {
                    ClientSocketOp::None | ClientSocketOp::New => {
                        *op = ClientSocketOp::Connect((Self::CONNECT_TIMEOUT, None));
                        debug!("<> Sending send_socket_connect to {:?}", sock);
                        self.manager
                            .send_socket_connect(*sock, addr)
                            .map_err(StackError::ConnectSendFailed)?;
                        Err(StackError::Dispatch)
                    }
                    ClientSocketOp::Connect((_, Some(connect_result))) => {
                        debug!("Connect result: {:?}", connect_result);
                        if connect_result.error == SocketError::NoError {
                            Ok(())
                        } else {
                            Err(StackError::OpFailed(connect_result.error))
                        }
                    }
                    ClientSocketOp::Connect((timeout, None)) => {
                        *timeout -= 1;
                        if *timeout == 0 {
                            Err(StackError::OpFailed(SocketError::Timeout))
                        } else {
                            Err(StackError::CallDelay)
                        }
                    }
                    _ => Err(StackError::Unexpected),
                };
                handle_result!(self, op, res)
            }
            core::net::SocketAddr::V6(_) => unimplemented!("IPv6 not supported"),
        }
    }
    fn send(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        data: &[u8],
    ) -> Result<usize, nb::Error<<Self as TcpClientStack>::Error>> {
        let (sock, op) = self.callbacks.tcp_sockets.get(*socket).unwrap();
        let res = match op {
            ClientSocketOp::None | ClientSocketOp::New => {
                debug!(
                    "<> Sending socket send_send to {:?} len:{}",
                    sock,
                    data.len()
                );
                let to_send = data.len().min(Self::MAX_SEND_LENGTH);
                let req = SendRequest {
                    offset: 0,
                    grand_total_sent: 0,
                    total_sent: 0,
                    remaining: to_send as i16,
                };
                debug!(
                    "Sending INITIAL send_send to {:?} len:{} req:{:?}",
                    sock, to_send, req
                );
                *op = ClientSocketOp::Send(req, None);
                self.manager
                    .send_send(*sock, &data[..to_send])
                    .map_err(StackError::SendSendFailed)?;
                Err(StackError::Dispatch)
            }
            // We finished one send iteration
            ClientSocketOp::Send(req, Some(len)) => {
                let total_sent = req.total_sent;
                let grand_total_sent = req.grand_total_sent;
                let new_offset = req.offset + total_sent as usize;
                // Now move to next chunk
                if new_offset >= data.len() {
                    debug!("Finished off a send, returning len:{}", *len as usize);
                    Ok(req.grand_total_sent as usize)
                } else {
                    let to_send = data[new_offset..].len().min(Self::MAX_SEND_LENGTH);
                    let new_req = SendRequest {
                        offset: new_offset,
                        grand_total_sent: grand_total_sent + total_sent,
                        total_sent: 0,
                        remaining: to_send as i16,
                    };
                    debug!(
                        "Sending NEXT send_send to {:?} len:{} req:{:?}",
                        sock, to_send, new_req
                    );
                    *op = ClientSocketOp::Send(new_req, None);
                    self.manager
                        .send_send(*sock, &data[new_offset..new_offset + to_send])
                        .map_err(StackError::SendSendFailed)?;
                    Err(StackError::Dispatch)
                }
            }
            // We are sending data, wait
            ClientSocketOp::Send(_, None) => Err(StackError::CallDelay),
            _ => Err(StackError::Unexpected),
        };
        handle_result!(self, op, res)
    }

    // Nb:: Blocking call, returns nb::Result when no data
    fn receive(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        data: &mut [u8],
    ) -> Result<usize, nb::Error<<Self as TcpClientStack>::Error>> {
        let (sock, op) = self.callbacks.tcp_sockets.get(*socket).unwrap();
        let res = match op {
            ClientSocketOp::None | ClientSocketOp::New => {
                *op = ClientSocketOp::Recv(None);
                debug!("<> Sending socket send_recv to {:?}", sock);
                self.manager
                    .send_recv(*sock, Self::RECV_TIMEOUT)
                    .map_err(|x| nb::Error::Other(StackError::ReceiveFailed(x)))?;
                Err(StackError::Dispatch)
            }
            ClientSocketOp::Recv(Some(recv_result)) => {
                debug!("Recv result: {:?}", recv_result);
                match recv_result.error {
                    SocketError::NoError => {
                        let recv_len = recv_result.recv_len;
                        let dest_slice = &mut data[..recv_len];
                        dest_slice.copy_from_slice(&self.callbacks.recv_buffer[..recv_len]);
                        Ok(recv_result.recv_len)
                    }
                    SocketError::Timeout => {
                        // Timeouts just get turned into a further wait
                        Err(StackError::CallDelay)
                    }
                    _ => Err(StackError::OpFailed(recv_result.error)),
                }
            }
            ClientSocketOp::Recv(None) => Err(StackError::CallDelay),
            _ => Err(StackError::Unexpected),
        };
        handle_result!(self, op, res)
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
        // Local server ports needs to be bound to 0.0.0.0
        let server_addr =
            core::net::SocketAddrV4::new(core::net::Ipv4Addr::new(0, 0, 0, 0), local_port);
        let (sock, op) = self.callbacks.tcp_sockets.get(*socket).unwrap();
        *op = ClientSocketOp::Bind(None);
        debug!("<> Sending TCP socket bind to {:?}", sock);
        self.manager
            .send_bind(*sock, server_addr)
            .map_err(StackError::BindFailed)?;
        self.wait_with_timeout(Self::BIND_TIMEOUT, |client, _| {
            let (_, op) = client.callbacks.tcp_sockets.get(*socket).unwrap();
            let res = match op {
                ClientSocketOp::Bind(Some(bind_result)) => match bind_result.error {
                    SocketError::NoError => Some(Ok(())),
                    _ => Some(Err(StackError::OpFailed(bind_result.error))),
                },
                _ => None,
            };
            if res.is_some() {
                *op = ClientSocketOp::None;
            }
            res
        })
    }

    fn listen(&mut self, socket: &mut Self::TcpSocket) -> Result<(), Self::Error> {
        let (sock, op) = self.callbacks.tcp_sockets.get(*socket).unwrap();
        *op = ClientSocketOp::Listen(None);
        debug!("<> Sending TCP socket listen to {:?}", sock);
        self.manager.send_listen(*sock, Self::TCP_SOCKET_BACKLOG)?;
        self.wait_with_timeout(Self::LISTEN_TIMEOUT, |client, _| {
            let (_, op) = client.callbacks.tcp_sockets.get(*socket).unwrap();
            let res = match op {
                ClientSocketOp::Listen(Some(listen_result)) => match listen_result.error {
                    SocketError::NoError => Some(Ok(())),
                    _ => Some(Err(StackError::OpFailed(listen_result.error))),
                },
                _ => None,
            };
            if res.is_some() {
                *op = ClientSocketOp::None;
            }
            res
        })
    }

    // This is a blocking call, return WouldBlock if no connection has been accepted
    fn accept(
        &mut self,
        socket: &mut Self::TcpSocket,
    ) -> nb::Result<(Self::TcpSocket, core::net::SocketAddr), Self::Error> {
        let (_, op) = self.callbacks.tcp_sockets.get(*socket).unwrap();
        let res = match op {
            ClientSocketOp::None | ClientSocketOp::New => {
                debug!("<> accept called on socket {:?}", socket);
                *op = ClientSocketOp::Accept(None);
                Err(StackError::Dispatch)
            }
            ClientSocketOp::Accept(Some(accept_result)) => {
                debug!("Accept result: {:?} on socket {:?}", accept_result, socket);
                Ok(*accept_result)
            }
            ClientSocketOp::Accept(None) => Err(StackError::CallDelay),
            _ => Err(StackError::Unexpected),
        };
        // Cant use this here, as there's more to do than just return the result
        //handle_result!(self, op, res)
        match res {
            Err(StackError::Dispatch) => {
                self.dispatch_events()?;
                Err(nb::Error::WouldBlock)
            }
            Err(StackError::CallDelay) => {
                self.delay(self.poll_loop_delay);
                self.dispatch_events()?;
                Err(nb::Error::WouldBlock)
            }
            Err(err) => {
                *op = ClientSocketOp::None;
                Err(nb::Error::Other(err))
            }
            Ok(accept_result) => {
                *op = ClientSocketOp::None;
                let accepted_socket = accept_result.accepted_socket;
                let handle = self
                    .callbacks
                    .tcp_sockets
                    .put(Handle(accepted_socket.v), accepted_socket.s)
                    .ok_or(StackError::SocketAlreadyInUse)?;
                Ok((handle, core::net::SocketAddr::V4(accept_result.accept_addr)))
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
