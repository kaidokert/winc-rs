use embedded_nal::TcpClientStack;

use super::WincClient;
use super::EventListener;
use super::Handle;
use super::StackError;
use super::ClientSocketOp;

impl<'a, X: wincwifi::transfer::Xfer, E: EventListener> embedded_nal::TcpClientStack
    for WincClient<'a, X, E>
{
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
                defmt::debug!("<> Sending send_socket_connect to {:?}", sock);
                self.manager
                    .send_socket_connect(*sock, addr)
                    .map_err(|x| StackError::ConnectSendFailed(x))?;
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
        let (sock, op) = self.callbacks.tcp_sockets.get(*socket).unwrap();
        *op = ClientSocketOp::Send;
        let op = *op;
        defmt::debug!("<> Sending socket send_send to {:?}", sock);
        self.manager
            .send_send(*sock, data)
            .map_err(|x| StackError::SendSendFailed(x))?;
        self.wait_for_op_ack(*socket, op, Self::SEND_TIMEOUT, true)?;
        Ok(data.len())
    }
    fn receive(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        data: &mut [u8],
    ) -> Result<usize, nb::Error<<Self as TcpClientStack>::Error>> {
        self.dispatch_events()?;
        let (sock, op) = self.callbacks.tcp_sockets.get(*socket).unwrap();
        *op = ClientSocketOp::Recv;
        let op = *op;
        let timeout = Self::RECV_TIMEOUT;
        defmt::debug!("<> Sending socket send_recv to {:?}", sock);
        self.manager
            .send_recv(*sock, timeout as u32)
            .map_err(|x| StackError::ReceiveFailed(x))?;
        let recv_len = self.wait_for_op_ack(*socket, op, self.recv_timeout, true)?;
        {
            let dest_slice = &mut data[..recv_len];
            dest_slice.copy_from_slice(&self.callbacks.recv_buffer[..recv_len]);
        }
        Ok(recv_len)
    }
    fn close(&mut self, socket: <Self as TcpClientStack>::TcpSocket) -> Result<(), Self::Error> {
        self.dispatch_events()?;
        let (sock, _op) = self.callbacks.tcp_sockets.get(socket).unwrap();
        self.manager
            .send_close(*sock)
            .map_err(|x| StackError::SendCloseFailed(x))?;
        self.callbacks
            .tcp_sockets
            .get(socket)
            .ok_or(StackError::CloseFailed)?;
        self.callbacks.tcp_sockets.remove(socket);
        Ok(())
    }
}

