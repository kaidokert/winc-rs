use embedded_nal::TcpClientStack;
use embedded_nal::TcpFullStack;

use super::ClientSocketOp;
use super::Handle;
use super::StackError;
use super::WincClient;

use super::Xfer;
use crate::manager::SocketError;
use crate::stack::socket_callbacks::SendRequest;
use crate::stack::socket_callbacks::{AsyncOp, AsyncState};
use crate::{debug, info};
use embedded_nal::nb;

use crate::stack::sock_holder::SocketStore;

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
        let res = match remote {
            core::net::SocketAddr::V4(addr) => Self::async_op(
                true,
                socket,
                &mut self.callbacks,
                &mut self.manager,
                self.poll_loop_delay_us,
                |op| matches!(op, AsyncOp::Connect(..)),
                |sock, manager| -> Result<ClientSocketOp, StackError> {
                    debug!("<> Sending send_socket_connect to {:?}", sock);
                    manager
                        .send_socket_connect(*sock, addr)
                        .map_err(StackError::ConnectSendFailed)?;
                    Ok(ClientSocketOp::AsyncOp(
                        AsyncOp::Connect(None),
                        AsyncState::Pending(Some(Self::CONNECT_TIMEOUT)),
                    ))
                },
                |_, _, _, asyncop| {
                    if let AsyncOp::Connect(Some(connect_result)) = asyncop {
                        if connect_result.error == SocketError::NoError {
                            Ok(())
                        } else {
                            Err(StackError::OpFailed(connect_result.error))
                        }
                    } else {
                        Err(StackError::Unexpected)
                    }
                },
            ),
            core::net::SocketAddr::V6(_) => unimplemented!("IPv6 not supported"),
        };
        self.test_hook();
        res
    }
    fn send(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        data: &[u8],
    ) -> Result<usize, nb::Error<<Self as TcpClientStack>::Error>> {
        let res = Self::async_op(
            true,
            socket,
            &mut self.callbacks,
            &mut self.manager,
            self.poll_loop_delay_us,
            |op| matches!(op, AsyncOp::Send(..)),
            |sock, manager| -> Result<ClientSocketOp, StackError> {
                let to_send = data.len().min(Self::MAX_SEND_LENGTH);
                let req = SendRequest {
                    offset: 0,
                    grand_total_sent: 0,
                    total_sent: 0,
                    remaining: to_send as i16,
                };
                debug!(
                    "Sending INITIAL send_send to {:?} len:{}/{} req:{:?}",
                    sock,
                    to_send,
                    data.len(),
                    req
                );
                manager
                    .send_send(*sock, &data[..to_send])
                    .map_err(StackError::SendSendFailed)?;
                Ok(ClientSocketOp::AsyncOp(
                    AsyncOp::Send(req, None),
                    AsyncState::Pending(None),
                ))
            },
            |sock, manager, _, asyncop| {
                if let AsyncOp::Send(req, Some(_len)) = asyncop {
                    let total_sent = req.total_sent;
                    let grand_total_sent = req.grand_total_sent + total_sent;
                    let offset = req.offset + total_sent as usize;
                    if offset >= data.len() {
                        Ok(grand_total_sent as usize)
                    } else {
                        let to_send = data[offset..].len().min(Self::MAX_SEND_LENGTH);
                        let new_req = SendRequest {
                            offset,
                            grand_total_sent,
                            total_sent: 0,
                            remaining: to_send as i16,
                        };
                        *asyncop = AsyncOp::Send(new_req, None);
                        manager
                            .send_send(*sock, &data[offset..offset + to_send])
                            .map_err(StackError::SendSendFailed)?;
                        Err(StackError::ContinueOperation)
                    }
                } else {
                    Err(StackError::Unexpected)
                }
            },
        );
        self.test_hook();
        res
    }

    // Nb:: Blocking call, returns nb::Result when no data
    // Todo: Bug: If a caller passes us a very large buffer that is larger than
    // max receive buffer, this should loop through serveral packets with
    // an offset - like send does.
    fn receive(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        data: &mut [u8],
    ) -> Result<usize, nb::Error<<Self as TcpClientStack>::Error>> {
        let res = Self::async_op(
            true,
            socket,
            &mut self.callbacks,
            &mut self.manager,
            self.poll_loop_delay_us,
            |op| matches!(op, AsyncOp::Recv(..)),
            |sock, manager| -> Result<ClientSocketOp, StackError> {
                debug!("<> Sending socket send_recv to {:?}", sock);
                manager
                    .send_recv(*sock, Self::RECV_TIMEOUT)
                    .map_err(StackError::ReceiveFailed)?;
                Ok(ClientSocketOp::AsyncOp(
                    AsyncOp::Recv(None),
                    AsyncState::Pending(None),
                ))
            },
            |sock, manager, recv_buffer, asyncop| {
                if let AsyncOp::Recv(Some(recv_result)) = asyncop {
                    match recv_result.error {
                        SocketError::NoError => {
                            let recv_len = recv_result.recv_len;
                            let dest_slice = &mut data[..recv_len];
                            dest_slice.copy_from_slice(&recv_buffer[..recv_len]);
                            Ok(recv_result.recv_len)
                        }
                        SocketError::Timeout => {
                            debug!("Timeout on receive, re-sending receive command");
                            // Re-send the receive command with the same timeout
                            manager
                                .send_recv(*sock, Self::RECV_TIMEOUT)
                                .map_err(StackError::ReceiveFailed)?;
                            Err(StackError::ContinueOperation)
                        }
                        _ => {
                            debug!("Error in receive: {:?}", recv_result.error);
                            Err(StackError::OpFailed(recv_result.error))
                        }
                    }
                } else {
                    Err(StackError::Unexpected)
                }
            },
        );
        self.test_hook();
        res
    }
    fn close(&mut self, socket: <Self as TcpClientStack>::TcpSocket) -> Result<(), Self::Error> {
        debug!("Closing socket {:?}", socket);
        self.dispatch_events()?;
        let (sock, _op) = self.callbacks.tcp_sockets.get(socket).unwrap();
        let socket_id = sock.v as usize;
        self.callbacks.listening_sockets[socket_id] = false;
        self.callbacks.accept_backlog[socket_id] = None;
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
        let sock_index = sock.v as usize;
        *op = ClientSocketOp::Listen(None);
        debug!("<> Sending TCP socket listen to {:?}", sock);
        self.manager.send_listen(*sock, Self::TCP_SOCKET_BACKLOG)?;
        let res = self.wait_with_timeout(Self::LISTEN_TIMEOUT, |client, _| {
            let (_, op) = client.callbacks.tcp_sockets.get(*socket).unwrap();
            let res = match op {
                ClientSocketOp::Listen(Some(listen_result)) => match listen_result.error {
                    // todo: here we have to mark successfully listening sockets, to deal with accept backlog
                    SocketError::NoError => Some(Ok(())),
                    _ => Some(Err(StackError::OpFailed(listen_result.error))),
                },
                _ => None,
            };
            if res.is_some() {
                *op = ClientSocketOp::None;
            }
            res
        });
        if res.is_ok() {
            self.callbacks.listening_sockets[sock_index] = true;
        }
        res
    }

    // This is a blocking call, return WouldBlock if no connection has been accepted
    fn accept(
        &mut self,
        socket: &mut Handle,
    ) -> nb::Result<(Handle, core::net::SocketAddr), StackError> {
        // Check if anything is backlogged
        for backlog in self.callbacks.accept_backlog.iter_mut() {
            if let Some((accepted_socket, addr)) = backlog.take() {
                info!("Accepting backlogged socket {:?}", accepted_socket);
                return Ok((Handle(accepted_socket.v), core::net::SocketAddr::V4(addr)));
            }
        }

        let res = Self::async_op(
            true,
            socket,
            &mut self.callbacks,
            &mut self.manager,
            self.poll_loop_delay_us,
            |op| matches!(op, AsyncOp::Accept(..)),
            |_, _| -> Result<ClientSocketOp, StackError> {
                debug!("<> accept called on socket {:?}", socket);
                // There's no manager.send_accept
                Ok(ClientSocketOp::AsyncOp(
                    AsyncOp::Accept(None),
                    AsyncState::Pending(None),
                ))
            },
            |_, _, _, asyncop| {
                if let AsyncOp::Accept(Some(accept_result)) = asyncop {
                    debug!("Accept result: {:?} on socket {:?}", accept_result, socket);
                    let accepted_socket = accept_result.accepted_socket;
                    let addr = accept_result.accept_addr;
                    Ok((accepted_socket, core::net::SocketAddr::V4(addr)))
                } else {
                    Err(StackError::Unexpected)
                }
            },
        );
        self.test_hook();
        match res {
            Ok((raw_socket, addr)) => {
                let handle = self
                    .callbacks
                    .tcp_sockets
                    .put(Handle(raw_socket.v), raw_socket.s)
                    .ok_or(StackError::SocketAlreadyInUse)?;
                Ok((handle, addr))
            }
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::client::{self, test_shared::*};
    use crate::{client::SocketCallbacks, manager::EventListener, socket::Socket};
    use core::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4};
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
            let result = client.connect(&mut tcp_socket, socket_addr);
            assert!(matches!(result, Err(nb::Error::WouldBlock)));
            counter += 1;
        }

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.connect(&mut tcp_socket, socket_addr));
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_tcp_send() {
        let mut client = make_test_client();
        let mut tcp_socket = client.socket().unwrap();
        let packet = "Hello, World";

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_send(
                Socket::new(0, 0),
                client::WincClient::<'_, MockTransfer>::MAX_SEND_LENGTH as i16,
            );
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
        assert_eq!(&recv_buff[..test_data.len()], test_data.as_bytes());
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

    #[test]
    fn test_tcp_check_max_send_buffer() {
        let mut client = make_test_client();
        let mut tcp_socket = client.socket().unwrap();
        let packet = "Hello, World";
        let socket = Socket::new(0, 0);
        let valid_len: i16 = client::WincClient::<'_, MockTransfer>::MAX_SEND_LENGTH as i16;

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_send(socket, valid_len);
        };

        client.debug_callback = Some(&mut my_debug);

        let result = client.send(&mut tcp_socket, packet.as_bytes());

        assert_eq!(result, Err(nb::Error::WouldBlock));

        if let Some((_, ClientSocketOp::AsyncOp(AsyncOp::Send(req, _), _))) =
            client.callbacks.resolve(socket)
        {
            assert!((req.total_sent == valid_len) && (req.remaining == 0 as i16));
        } else {
            assert!(false, "Expected Some value, but it returned None");
        }
    }

    #[test]
    fn test_tcp_check_receive_timeout() {
        let mut client = make_test_client();
        let mut tcp_socket = client.socket().unwrap();
        let socket_addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 80);
        let mut recv_buff = [0u8; 32];
        let mut counter = 5;

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_recv(Socket::new(0, 0), socket_addr, &[], SocketError::Timeout);
        };

        client.debug_callback = Some(&mut my_debug);

        while counter != 0 {
            let result = client.receive(&mut tcp_socket, &mut recv_buff);

            assert_eq!(result.err(), Some(nb::Error::WouldBlock));
            counter -= 1;
        }
    }

    #[test]
    fn test_tcp_check_receive_err() {
        let mut client = make_test_client();
        let mut tcp_socket = client.socket().unwrap();
        let socket_addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 80);
        let mut recv_buff = [0u8; 32];

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_recv(
                Socket::new(0, 0),
                socket_addr,
                &[],
                SocketError::ConnAborted,
            );
        };

        client.debug_callback = Some(&mut my_debug);

        let result = nb::block!(client.receive(&mut tcp_socket, &mut recv_buff));

        assert_eq!(
            result.err(),
            Some(StackError::OpFailed(SocketError::ConnAborted))
        );
    }

    #[test]
    fn test_tcp_check_bind_error() {
        let mut client = make_test_client();
        let mut tcp_socket = client.socket().unwrap();

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_bind(Socket::new(0, 0), SocketError::InvalidAddress);
        };

        client.debug_callback = Some(&mut my_debug);

        let result = client.bind(&mut tcp_socket, 8080);

        assert_eq!(
            result.err(),
            Some(StackError::OpFailed(SocketError::InvalidAddress))
        );
    }

    #[test]
    fn test_tcp_check_accept_backlog() {
        let mut client = make_test_client();
        let mut tcp_socket = client.socket().unwrap();
        let socket_addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 80);
        let socket = Socket::new(0, 0);

        client.callbacks.accept_backlog[0] = Some((socket, socket_addr));

        let result = nb::block!(client.accept(&mut tcp_socket));

        assert_eq!(
            result.ok(),
            Some((Handle(0), core::net::SocketAddr::V4(socket_addr)))
        );
    }

    #[test]
    fn test_tcp_check_listen_error() {
        let mut client = make_test_client();
        let mut tcp_socket = client.socket().unwrap();

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_listen(Socket::new(0, 0), SocketError::ConnAborted);
        };

        client.debug_callback = Some(&mut my_debug);

        let result = client.listen(&mut tcp_socket);

        assert!(result.is_err());
    }

    #[test]
    fn test_tcp_connect_error() {
        let mut client = make_test_client();
        let mut tcp_socket = client.socket().unwrap();

        let mut my_debug = |callbacks: &mut SocketCallbacks| {
            callbacks.on_connect(Socket::new(0, 0), SocketError::ConnAborted);
        };

        client.debug_callback = Some(&mut my_debug);
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 80);

        let result = nb::block!(client.connect(&mut tcp_socket, socket_addr));

        assert_eq!(
            result.err(),
            Some(StackError::OpFailed(SocketError::ConnAborted))
        );
    }

    #[test]
    #[should_panic]
    fn test_tcp_connect_ipv6() {
        let mut client = make_test_client();
        let mut tcp_socket = client.socket().unwrap();

        let socket_addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0)), 80);

        let _ = client.connect(&mut tcp_socket, socket_addr);
    }
}
