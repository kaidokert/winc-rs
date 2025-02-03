use super::WincClient;
use super::EventListener;
use super::Handle;
use super::StackError;
use super::ClientSocketOp;
use embedded_nal::UdpClientStack;

impl<'a, X: wincwifi::transfer::Xfer, E: EventListener> UdpClientStack for WincClient<'a, X, E> {
    type UdpSocket = Handle;

    type Error = StackError;

    fn socket(&mut self) -> Result<Self::UdpSocket, Self::Error> {
        defmt::debug!("<> Calling new UDP socket");
        self.dispatch_events()?;
        let s = self.get_next_session_id();
        let handle = self
            .callbacks
            .udp_sockets
            .add(s)
            .ok_or(StackError::OutOfSockets)?;
        defmt::debug!("<> Got handle {:?} ", handle.0);
        Ok(handle)
    }

    fn connect(
        &mut self,
        socket: &mut Self::UdpSocket,
        remote: core::net::SocketAddr,
    ) -> Result<(), Self::Error> {
        self.dispatch_events()?;
        match remote {
            core::net::SocketAddr::V4(addr) => {
                defmt::debug!("<> Connect handle is {:?}", socket.0);
                let (_sock, _op) = self.callbacks.udp_sockets.get(*socket).unwrap();
                self.last_send_addr = Some(addr);
            }
            core::net::SocketAddr::V6(_) => unimplemented!("IPv6 not supported"),
        }
        Ok(())
    }

    fn send(&mut self, socket: &mut Self::UdpSocket, buffer: &[u8]) -> nb::Result<(), Self::Error> {
        self.dispatch_events()?;
        defmt::debug!("<> in udp send {:?}", socket.0);
        let (sock, op) = self.callbacks.udp_sockets.get(*socket).unwrap();
        *op = ClientSocketOp::SendTo;
        let op = *op;
        defmt::debug!("<> Sending socket udp send_send to {:?}", sock);
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
        defmt::debug!("<> Sending udp socket send_recv to {:?}", sock);
        self.manager
            .send_recvfrom(*sock, timeout)
            .map_err(|x| StackError::ReceiveFailed(x))?;
        let recv_len = self.wait_for_op_ack(*socket, op, self.recv_timeout, false)?;
        {
            let dest_slice = &mut buffer[..recv_len];
            dest_slice.copy_from_slice(&self.callbacks.recv_buffer[..recv_len]);
        }
        let f = self.last_send_addr.unwrap();
        Ok((recv_len, core::net::SocketAddr::V4(f)))
    }

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
