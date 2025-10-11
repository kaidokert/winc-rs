use crate::StackError;
use embedded_nal_async::TcpConnect;
use super::AsyncClient;
use crate::transfer::Xfer;
use crate::Handle;

// TODO: Not sure this should be public
pub struct AsyncTcpConnection<'a, X: Xfer> {
    client: &'a AsyncClient<'a, X>,
    socket: Handle,
}


// Implement embedded-io-async traits for AsyncTcpConnection
impl<X: Xfer> embedded_io_async::ErrorType for AsyncTcpConnection<'_, X> {
    type Error = StackError;
}

impl<X: Xfer> embedded_io_async::Read for AsyncTcpConnection<'_, X> {
    async fn read(&mut self, _buf: &mut [u8]) -> Result<usize, Self::Error> {
        todo!("Async TCP read not yet implemented")
    }
}

impl<X: Xfer> embedded_io_async::Write for AsyncTcpConnection<'_, X> {
    async fn write(&mut self, _buf: &[u8]) -> Result<usize, Self::Error> {
        todo!("Async TCP write not yet implemented")
    }
}

impl<X: Xfer> TcpConnect for AsyncClient<'_, X> {
    type Error = StackError;
    type Connection<'a> = AsyncTcpConnection<'a, X> where Self: 'a;

    async fn connect<'a>(&'a self, _remote: core::net::SocketAddr) -> Result<Self::Connection<'a>, Self::Error> {
        todo!("Async TCP connect not yet implemented - use synchronous client instead")
    }
}
