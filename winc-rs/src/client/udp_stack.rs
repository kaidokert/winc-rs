use super::ClientSocketOp;
use super::Handle;
use super::StackError;
use super::WincClient;
use embedded_nal::UdpClientStack;
use embedded_nal::UdpFullStack;

use super::Xfer;

use crate::debug;
use crate::manager::SocketError;
use crate::stack::socket_callbacks::UDP_SOCK_OFFSET;
use embedded_nal::nb;

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

    // Blocking call ? returns nb::Result
    fn send(&mut self, socket: &mut Self::UdpSocket, buffer: &[u8]) -> nb::Result<(), Self::Error> {
        self.dispatch_events()?;
        let mut offset = 0;

        while offset < buffer.len() {
            let to_send = buffer[offset..].len().min(Self::MAX_SEND_LENGTH);

            let (sock, op) = self.callbacks.udp_sockets.get(*socket).unwrap();
            *op = ClientSocketOp::SendTo(buffer.len() as i16);
            let op = *op;
            debug!("<> Sending socket udp send_send to {:?}", sock);
            if let Some(addr) =
                self.callbacks.udp_socket_connect_addr[sock.v as usize - UDP_SOCK_OFFSET]
            {
                self.manager
                    .send_sendto(*sock, addr, buffer)
                    .map_err(StackError::SendSendFailed)?;
            } else {
                return Err(StackError::Unexpected.into());
            }
            self.wait_for_op_ack(*socket, op, Self::SEND_TIMEOUT, false)?;
            offset += to_send;
        }
        Ok(())
    }

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
                if recv_result.error == SocketError::NoError {
                    let recv_len = recv_result.recv_len;
                    let dest_slice = &mut buffer[..recv_len];
                    dest_slice.copy_from_slice(&self.callbacks.recv_buffer[..recv_len]);
                    Ok((
                        recv_result.recv_len,
                        core::net::SocketAddr::V4(recv_result.from_addr),
                    ))
                } else {
                    Err(StackError::OpFailed(recv_result.error))
                }
            }
            ClientSocketOp::RecvFrom(None) => Err(StackError::CallDelay),
            _ => Err(StackError::Unexpected),
        };
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
            Ok(result) => {
                *op = ClientSocketOp::None;
                Ok(result)
            }
        }
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

    fn send_to(
        &mut self,
        socket: &mut Self::UdpSocket,
        remote: core::net::SocketAddr,
        buffer: &[u8],
    ) -> nb::Result<(), Self::Error> {
        self.dispatch_events()?;
        let send_addr = match remote {
            core::net::SocketAddr::V4(addr) => {
                debug!("<> Connect handle is {:?}", socket.0);
                let (_sock, _op) = self.callbacks.udp_sockets.get(*socket).unwrap();
                addr
            }
            core::net::SocketAddr::V6(_) => unimplemented!("IPv6 not supported"),
        };

        debug!("<> in udp send_to {:?}", socket.0);
        let (sock, op) = self.callbacks.udp_sockets.get(*socket).unwrap();
        *op = ClientSocketOp::SendTo(buffer.len() as i16);
        let op = *op;
        self.manager
            .send_sendto(*sock, send_addr, buffer)
            .map_err(StackError::SendSendFailed)?;
        self.wait_for_op_ack(*socket, op, Self::SEND_TIMEOUT, false)?;
        Ok(())
    }
}
