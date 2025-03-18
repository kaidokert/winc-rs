use core::net::SocketAddrV4;

use super::ClientSocketOp;
use super::Handle;
use super::StackError;
use super::WincClient;
use embedded_nal::UdpClientStack;
use embedded_nal::UdpFullStack;

use super::Xfer;

use crate::debug;
use crate::manager::SocketError;
use crate::stack::socket_callbacks::AsyncOp;
use crate::stack::socket_callbacks::AsyncState;
use crate::stack::socket_callbacks::SendRequest;
use crate::stack::socket_callbacks::UDP_SOCK_OFFSET;
use embedded_nal::nb;

use crate::stack::sock_holder::SocketStore;

impl<X: Xfer> WincClient<'_, X> {
    fn send_udp_inner(
        &mut self,
        socket: &mut Handle,
        addr: SocketAddrV4,
        data: &[u8],
    ) -> nb::Result<(), StackError> {
        let res = Self::generic_op(
            false,
            socket,
            &mut self.callbacks,
            &mut self.manager,
            self.poll_loop_delay_us,
            |op| matches!(op, AsyncOp::SendTo(..)),
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
                manager.send_sendto(*sock, addr, &data[..to_send])?;
                Ok(ClientSocketOp::AsyncOp(
                    AsyncOp::SendTo(req, None),
                    AsyncState::Pending(None),
                ))
            },
            |sock, manager, _, asyncop| {
                if let AsyncOp::SendTo(req, Some(_len)) = asyncop {
                    let total_sent = req.total_sent;
                    let grand_total_sent = req.grand_total_sent + total_sent;
                    let offset = req.offset + total_sent as usize;
                    if offset >= data.len() {
                        Ok(())
                    } else {
                        let to_send = data[offset..].len().min(Self::MAX_SEND_LENGTH);
                        let new_req = SendRequest {
                            offset,
                            grand_total_sent,
                            total_sent: 0,
                            remaining: to_send as i16,
                        };
                        *asyncop = AsyncOp::SendTo(new_req, None);
                        manager.send_sendto(*sock, addr, &data[offset..offset + to_send])?;
                        Err(StackError::ContinueOperation)
                    }
                } else {
                    Err(StackError::Unexpected)
                }
            },
        );
        self.debughook();
        res
    }

    // Todo: consider consolidating this with TCP
}

impl<X: Xfer> UdpClientStack for WincClient<'_, X> {
    type UdpSocket = Handle;

    type Error = StackError;

    fn socket(&mut self) -> Result<Self::UdpSocket, Self::Error> {
        debug!("<> Calling new UDP socket");
        self.dispatch_events()?;
        let s = self.get_next_session_id();
        let handle = self
            .callbacks
            .udp_sockets
            .add(s)
            .ok_or(StackError::OutOfSockets)?;
        debug!("<> Got handle {:?} ", handle.0);
        Ok(handle)
    }

    // Not a blocking call
    fn connect(
        &mut self,
        socket: &mut Self::UdpSocket,
        remote: core::net::SocketAddr,
    ) -> Result<(), Self::Error> {
        self.dispatch_events()?;
        match remote {
            core::net::SocketAddr::V4(addr) => {
                debug!("<> Connect handle is {:?}", socket.0);
                let (sock, _op) = self.callbacks.udp_sockets.get(*socket).unwrap();
                self.callbacks.udp_socket_connect_addr[sock.v as usize - UDP_SOCK_OFFSET] =
                    Some(addr);
            }
            core::net::SocketAddr::V6(_) => unimplemented!("IPv6 not supported"),
        }
        Ok(())
    }

    fn send(&mut self, socket: &mut Self::UdpSocket, data: &[u8]) -> nb::Result<(), Self::Error> {
        let addr = {
            // temporary borrow
            let (sock, _op) = self
                .callbacks
                .udp_sockets
                .get(*socket)
                .ok_or(StackError::SocketNotFound)?;
            self.callbacks.udp_socket_connect_addr[sock.v as usize - UDP_SOCK_OFFSET]
                .ok_or(StackError::Unexpected)?
        };
        self.send_udp_inner(socket, addr, data)
    }

    // Todo: consider consolidating this with TCP
    // Todo: Bug: If a caller passes us a very large buffer that is larger than
    // max receive buffer, this should loop through serveral packets with
    // an offset - like send does.
    fn receive(
        &mut self,
        socket: &mut Self::UdpSocket,
        buffer: &mut [u8],
    ) -> nb::Result<(usize, core::net::SocketAddr), Self::Error> {
        let res = Self::generic_op(
            false,
            socket,
            &mut self.callbacks,
            &mut self.manager,
            self.poll_loop_delay_us,
            |op| matches!(op, AsyncOp::RecvFrom(..)),
            |sock, manager| -> Result<ClientSocketOp, StackError> {
                debug!("<> Sending udp socket send_recv to {:?}", sock);
                manager.send_recvfrom(*sock, Self::RECV_TIMEOUT)?;
                Ok(ClientSocketOp::AsyncOp(
                    AsyncOp::RecvFrom(None),
                    AsyncState::Pending(None),
                ))
            },
            |_, _, recv_buffer, asyncop| {
                if let AsyncOp::RecvFrom(Some(recv_result)) = asyncop {
                    match recv_result.error {
                        SocketError::NoError => {
                            let recv_len = recv_result.recv_len;
                            let dest_slice = &mut buffer[..recv_len];
                            dest_slice.copy_from_slice(&recv_buffer[..recv_len]);
                            Ok((
                                recv_result.recv_len,
                                core::net::SocketAddr::V4(recv_result.from_addr),
                            ))
                        }
                        SocketError::Timeout => {
                            debug!("Timeout on receive");
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
        self.debughook();
        res
    }

    // Not a blocking call
    fn close(&mut self, socket: Self::UdpSocket) -> Result<(), Self::Error> {
        self.dispatch_events()?;
        let (sock, _op) = self.callbacks.udp_sockets.get(socket).unwrap();
        let sock_id = sock.v;
        self.manager
            .send_close(*sock)
            .map_err(StackError::SendCloseFailed)?;
        self.callbacks
            .udp_sockets
            .get(socket)
            .ok_or(StackError::CloseFailed)?;
        self.callbacks.udp_sockets.remove(socket);
        // clear send addresses
        self.callbacks.udp_socket_connect_addr[sock_id as usize - UDP_SOCK_OFFSET] = None;
        Ok(())
    }
}

impl<X: Xfer> UdpFullStack for WincClient<'_, X> {
    // Not a blocking call
    fn bind(&mut self, socket: &mut Self::UdpSocket, local_port: u16) -> Result<(), Self::Error> {
        // Local server ports needs to be bound to 0.0.0.0
        let server_addr =
            core::net::SocketAddrV4::new(core::net::Ipv4Addr::new(0, 0, 0, 0), local_port);
        let (sock, op) = self.callbacks.udp_sockets.get(*socket).unwrap();
        *op = ClientSocketOp::Bind(None);
        debug!("<> Sending UDP socket bind to {:?}", sock);
        self.manager
            .send_bind(*sock, server_addr)
            .map_err(StackError::BindFailed)?;
        self.wait_with_timeout(Self::BIND_TIMEOUT, |client, _| {
            let (_, op) = client.callbacks.udp_sockets.get(*socket).unwrap();
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

    // Todo: Reduce copy-paste between send and send_to implementations
    fn send_to(
        &mut self,
        socket: &mut Self::UdpSocket,
        remote: core::net::SocketAddr,
        data: &[u8],
    ) -> nb::Result<(), Self::Error> {
        let addr = match remote {
            core::net::SocketAddr::V4(addr) => addr,
            core::net::SocketAddr::V6(_) => unimplemented!("IPv6 not supported"),
        };
        self.send_udp_inner(socket, addr, data)
    }
}
