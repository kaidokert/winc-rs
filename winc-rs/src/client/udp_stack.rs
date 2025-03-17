use super::ClientSocketOp;
use super::Handle;
use super::StackError;
use super::WincClient;
use embedded_nal::UdpClientStack;
use embedded_nal::UdpFullStack;

use super::Xfer;

use crate::debug;
use crate::manager::SocketError;
use crate::stack::socket_callbacks::SendRequest;
use crate::stack::socket_callbacks::UDP_SOCK_OFFSET;
use embedded_nal::nb;

use crate::handle_result;

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
        let (sock, op) = self.callbacks.udp_sockets.get(*socket).unwrap();
        let addr = self.callbacks.udp_socket_connect_addr[sock.v as usize - UDP_SOCK_OFFSET]
            .ok_or(StackError::Unexpected)?;
        let res = match op {
            ClientSocketOp::None | ClientSocketOp::New => {
                debug!(
                    "<> Sending socket udp send_send to {:?} len:{}",
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
                *op = ClientSocketOp::SendTo(req, None);
                self.manager
                    .send_sendto(*sock, addr, &data[..to_send])
                    .map_err(StackError::SendSendFailed)?;
                Err(StackError::Dispatch)
            }
            ClientSocketOp::SendTo(req, Some(_len)) => {
                let total_sent = req.total_sent;
                let grand_total_sent = req.grand_total_sent + total_sent;
                let offset = req.offset + total_sent as usize;
                // Now move to next chunk
                if offset >= data.len() {
                    debug!("Finished off a send, returning len:{}", grand_total_sent);
                    Ok(())
                } else {
                    let to_send = data[offset..].len().min(Self::MAX_SEND_LENGTH);
                    let new_req = SendRequest {
                        offset,
                        grand_total_sent,
                        total_sent: 0,
                        remaining: to_send as i16,
                    };
                    debug!(
                        "Sending NEXT send_send to {:?} len:{} req:{:?}",
                        sock, to_send, new_req
                    );
                    *op = ClientSocketOp::SendTo(new_req, None);
                    self.manager
                        .send_sendto(*sock, addr, &data[offset..offset + to_send])
                        .map_err(StackError::SendSendFailed)?;
                    Err(StackError::Dispatch)
                }
            }
            ClientSocketOp::SendTo(_, None) => Err(StackError::CallDelay),
            _ => Err(StackError::Unexpected),
        };
        handle_result!(self, op, res)
    }

    // Todo: Bug: If a caller passes us a very large buffer that is larger than
    // max receive buffer, this should loop through serveral packets with
    // an offset - like send does.
    fn receive(
        &mut self,
        socket: &mut Self::UdpSocket,
        buffer: &mut [u8],
    ) -> nb::Result<(usize, core::net::SocketAddr), Self::Error> {
        let (sock, op) = self.callbacks.udp_sockets.get(*socket).unwrap();
        let res = match op {
            ClientSocketOp::None | ClientSocketOp::New => {
                *op = ClientSocketOp::RecvFrom(None);
                debug!("<> Sending udp socket send_recv to {:?}", sock);
                self.manager
                    .send_recvfrom(*sock, Self::RECV_TIMEOUT)
                    .map_err(StackError::ReceiveFailed)?;
                Err(StackError::Dispatch)
            }
            ClientSocketOp::RecvFrom(Some(recv_result)) => {
                debug!("Recv result: {:?}", recv_result);
                match recv_result.error {
                    SocketError::NoError => {
                        let recv_len = recv_result.recv_len;
                        let dest_slice = &mut buffer[..recv_len];
                        dest_slice.copy_from_slice(&self.callbacks.recv_buffer[..recv_len]);
                        Ok((
                            recv_result.recv_len,
                            core::net::SocketAddr::V4(recv_result.from_addr),
                        ))
                    }
                    SocketError::Timeout => Err(StackError::CallDelay),
                    _ => Err(StackError::OpFailed(recv_result.error)),
                }
            }
            ClientSocketOp::RecvFrom(None) => Err(StackError::CallDelay),
            _ => Err(StackError::Unexpected),
        };
        handle_result!(self, op, res)
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
            core::net::SocketAddr::V4(addr) => {
                debug!("<> Connect handle is {:?}", socket.0);
                let (_sock, _op) = self.callbacks.udp_sockets.get(*socket).unwrap();
                addr
            }
            core::net::SocketAddr::V6(_) => unimplemented!("IPv6 not supported"),
        };
        let (sock, op) = self.callbacks.udp_sockets.get(*socket).unwrap();
        let res = match op {
            ClientSocketOp::None | ClientSocketOp::New => {
                debug!("<> Sending UDP send_to {:?} len:{}", sock, data.len());
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
                *op = ClientSocketOp::SendTo(req, None);
                self.manager
                    .send_sendto(*sock, addr, &data[..to_send])
                    .map_err(StackError::SendSendFailed)?;
                Err(StackError::Dispatch)
            }
            ClientSocketOp::SendTo(req, Some(_len)) => {
                let total_sent = req.total_sent;
                let grand_total_sent = req.grand_total_sent + total_sent;
                let offset = req.offset + total_sent as usize;
                // Now move to next chunk
                if offset >= data.len() {
                    debug!("Finished off a send, total was len:{}", grand_total_sent);
                    Ok(())
                } else {
                    let to_send = data[offset..].len().min(Self::MAX_SEND_LENGTH);
                    let new_req = SendRequest {
                        offset,
                        grand_total_sent,
                        total_sent: 0,
                        remaining: to_send as i16,
                    };
                    debug!(
                        "Sending NEXT send_send to {:?} len:{} req:{:?}",
                        sock, to_send, new_req
                    );
                    *op = ClientSocketOp::SendTo(new_req, None);
                    self.manager
                        .send_sendto(*sock, addr, &data[offset..offset + to_send])
                        .map_err(StackError::SendSendFailed)?;
                    Err(StackError::Dispatch)
                }
            }
            ClientSocketOp::SendTo(_, None) => Err(StackError::CallDelay),
            _ => Err(StackError::Unexpected),
        };
        handle_result!(self, op, res)
    }
}
