use embedded_nal::UdpClientStack;

use crate::{client::UdpSocket, WincClient};

impl UdpClientStack for WincClient {
    type UdpSocket = u32;
    type Error = u32;
    fn socket(&mut self) -> Result<Self::UdpSocket, Self::Error> {
        if let Some(ref mut sockets) = self.sockets {
            if sockets.len() >= sockets.capacity() {
                return Err(2);
            }
            let s = UdpSocket {};
            sockets.add(s).map(|_| 0).map_err(|_| 3)
        } else {
            Err(1)
        }
    }
    fn connect(
        &mut self,
        _socket: &mut Self::UdpSocket,
        _remote: no_std_net::SocketAddr,
    ) -> Result<(), Self::Error> {
        todo!()
    }
    fn receive(
        &mut self,
        _socket: &mut Self::UdpSocket,
        _buffer: &mut [u8],
    ) -> embedded_nal::nb::Result<(usize, no_std_net::SocketAddr), Self::Error> {
        todo!()
    }
    fn close(&mut self, _socket: Self::UdpSocket) -> Result<(), Self::Error> {
        todo!()
    }
    fn send(
        &mut self,
        _socket: &mut Self::UdpSocket,
        _buffer: &[u8],
    ) -> embedded_nal::nb::Result<(), Self::Error> {
        todo!()
    }
}
