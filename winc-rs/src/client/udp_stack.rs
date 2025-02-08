use super::ClientSocketOp;
use super::GenResult;
use super::Handle;
use super::StackError;
use super::WincClient;
use embedded_nal::UdpClientStack;
use embedded_nal::UdpFullStack;

use super::Xfer;

use crate::debug;
use crate::manager::SocketError;
use embedded_nal::nb;

impl<'a, X: Xfer> UdpClientStack for WincClient<'a, X> {
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
                let (_sock, _op) = self.callbacks.udp_sockets.get(*socket).unwrap();
                self.last_send_addr = Some(addr);
            }
            core::net::SocketAddr::V6(_) => unimplemented!("IPv6 not supported"),
        }
        Ok(())
    }

    // Blocking call ? returns nb::Result
    fn send(&mut self, socket: &mut Self::UdpSocket, buffer: &[u8]) -> nb::Result<(), Self::Error> {
        self.dispatch_events()?;
        debug!("<> in udp send {:?}", socket.0);
        let (sock, op) = self.callbacks.udp_sockets.get(*socket).unwrap();
        *op = ClientSocketOp::SendTo;
        let op = *op;
        debug!("<> Sending socket udp send_send to {:?}", sock);
        if let Some(addr) = self.last_send_addr {
            self.manager
                .send_sendto(*sock, addr, buffer)
                .map_err(|x| StackError::SendSendFailed(x))?;
        } else {
            return Err(StackError::Unexpected.into());
        }
        self.wait_for_op_ack(*socket, op, Self::SEND_TIMEOUT, false)?;
        Ok(())
    }

    // TODO: We should return WouldBlock if there's no data
    fn receive(
        &mut self,
        socket: &mut Self::UdpSocket,
        buffer: &mut [u8],
    ) -> nb::Result<(usize, core::net::SocketAddr), Self::Error> {
        self.dispatch_events()?;
        let (sock, op) = self.callbacks.udp_sockets.get(*socket).unwrap();
        *op = ClientSocketOp::RecvFrom;
        let op = *op;
        let timeout = Self::RECV_TIMEOUT;
        debug!("<> Sending udp socket send_recv to {:?}", sock);
        self.manager
            .send_recvfrom(*sock, timeout)
            .map_err(|x| StackError::ReceiveFailed(x))?;
        if let GenResult::Len(recv_len) =
            match self.wait_for_op_ack(*socket, op, self.recv_timeout, false) {
                Ok(result) => result,
                Err(StackError::OpFailed(SocketError::Timeout)) => {
                    return Err(nb::Error::WouldBlock)
                }
                Err(e) => return Err(nb::Error::Other(e)),
            }
        {
            let dest_slice = &mut buffer[..recv_len];
            dest_slice.copy_from_slice(&self.callbacks.recv_buffer[..recv_len]);
            let f = self.last_send_addr.unwrap();
            Ok((recv_len, core::net::SocketAddr::V4(f)))
        } else {
            Err(nb::Error::Other(StackError::Unexpected))
        }
    }

    // Not a blocking call
    fn close(&mut self, socket: Self::UdpSocket) -> Result<(), Self::Error> {
        self.dispatch_events()?;
        let (sock, _op) = self.callbacks.udp_sockets.get(socket).unwrap();
        self.manager
            .send_close(*sock)
            .map_err(|x| StackError::SendCloseFailed(x))?;
        self.callbacks
            .udp_sockets
            .get(socket)
            .ok_or(StackError::CloseFailed)?;
        self.callbacks.udp_sockets.remove(socket);
        Ok(())
    }
}

impl<'a, X: Xfer> UdpFullStack for WincClient<'a, X> {
    // Not a blocking call
    fn bind(&mut self, socket: &mut Self::UdpSocket, local_port: u16) -> Result<(), Self::Error> {
        // Local server ports needs to be bound to 0.0.0.0
        let server_addr =
            core::net::SocketAddrV4::new(core::net::Ipv4Addr::new(0, 0, 0, 0), local_port);
        let (sock, op) = self.callbacks.udp_sockets.get(*socket).unwrap();
        *op = ClientSocketOp::Bind;
        let op = *op;
        self.manager
            .send_bind(*sock, server_addr)
            .map_err(|x| StackError::BindFailed(x))?;
        self.wait_for_op_ack(*socket, op, Self::BIND_TIMEOUT, false)?;
        Ok(())
    }

    // TODO: Blocking call, returns nb::Result, handle similar to send()
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
        *op = ClientSocketOp::SendTo;
        let op = *op;
        self.manager
            .send_sendto(*sock, send_addr, buffer)
            .map_err(|x| StackError::SendSendFailed(x))?;
        self.wait_for_op_ack(*socket, op, Self::SEND_TIMEOUT, false)?;
        Ok(())
    }
}
