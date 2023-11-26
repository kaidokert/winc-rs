use arrayvec::ArrayVec;

pub struct UdpSocket {}
pub struct TcpSocket {}

pub enum LocalErrors {}

impl From<UdpSocket> for Socket {
    fn from(v: UdpSocket) -> Self {
        Self::Udp(v)
    }
}

pub enum Socket {
    Udp(UdpSocket),
    Tcp(TcpSocket),
}

pub struct SocketSet<const N: usize> {
    pub sockets: ArrayVec<Option<Socket>, N>,
}

impl<const N: usize> Default for SocketSet<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> SocketSet<N> {
    pub fn new() -> Self {
        Self {
            sockets: ArrayVec::new(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.sockets.is_empty()
    }
    pub fn len(&self) -> usize {
        self.sockets.len()
    }
    pub fn capacity(&self) -> usize {
        N
    }
    pub fn add<S>(&mut self, _socket: S) -> core::result::Result<u32, LocalErrors>
    where
        S: Into<Socket>,
    {
        Ok(1)
    }
}

pub struct WincClient {
    pub sockets: Option<&'static mut SocketSet<10>>,
}

impl WincClient {
    pub fn new() -> Self {
        Self { sockets: None }
    }
    pub fn set_socket_storage(&mut self, socket_set: &'static mut SocketSet<10>) {
        self.sockets.replace(socket_set);
    }
}

impl Default for WincClient {
    fn default() -> Self {
        Self::new()
    }
}
