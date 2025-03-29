use super::ClientSocketOp;
use super::Handle;
use super::Socket;

pub struct SockHolder<const N: usize, const BASE: usize> {
    sockets: [Option<(Socket, ClientSocketOp)>; N],
}

impl<const N: usize, const BASE: usize> Default for SockHolder<N, BASE> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize, const BASE: usize> SockHolder<N, BASE> {
    pub fn new() -> Self {
        Self {
            sockets: core::array::from_fn(|_| None),
        }
    }
    fn len(&self) -> usize {
        self.sockets.iter().filter(|a| a.is_some()).count()
    }
}

pub(crate) trait SocketStore {
    fn add(&mut self, session_id: u16) -> Option<Handle>;
    fn remove(&mut self, handle: Handle);
    fn get(&mut self, handle: Handle) -> Option<&mut (Socket, ClientSocketOp)>;
    fn put(&mut self, handle: Handle, session_id: u16) -> Option<Handle>;
}

impl<const N: usize, const BASE: usize> SocketStore for SockHolder<N, BASE> {
    fn add(&mut self, session_id: u16) -> Option<Handle> {
        if self.len() >= N {
            return None;
        }
        for (index, element) in self.sockets.iter_mut().enumerate() {
            if element.is_none() {
                let ns = Socket::new((BASE + index) as u8, session_id);
                element.replace((ns, ClientSocketOp::New));
                return Some(Handle(index as u8));
            }
        }
        None
    }
    fn remove(&mut self, handle: Handle) {
        self.sockets[handle.0 as usize] = None;
    }
    fn put(&mut self, handle: Handle, session_id: u16) -> Option<Handle> {
        if self.len() >= N {
            return None;
        }
        // First check if this index is occupied
        if self.sockets[handle.0 as usize].is_some() {
            return None;
        }
        self.sockets[handle.0 as usize] =
            Some((Socket::new(handle.0, session_id), ClientSocketOp::New));
        Some(handle)
    }
    fn get(&mut self, handle: Handle) -> Option<&mut (Socket, ClientSocketOp)> {
        self.sockets[handle.0 as usize].as_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_containers() {
        let mut socks = SockHolder::<2, 7>::new();
        let handle0 = socks.add(13).unwrap();
        let (s, _) = socks.get(handle0).unwrap();
        assert_eq!(s.v, 7);
        assert_eq!(s.s, 13);
        let handle1 = socks.add(42).unwrap();
        let (s, _) = socks.get(handle1).unwrap();
        assert_eq!(s.v, 8);
        assert_eq!(s.s, 42);
        assert_eq!(socks.add(42), None);
        socks.remove(handle0);
        let handle2 = socks.add(50).unwrap();
        let (s, _) = socks.get(handle2).unwrap();
        assert_eq!(s.v, 7);
        assert_eq!(s.s, 50);
    }
    #[test]
    fn test_mixmatch() {
        let mut tcp_sockets: SockHolder<7, 0> = SockHolder::new();
        let mut udp_sockets: SockHolder<4, 7> = SockHolder::new();
        let tcp_sock = tcp_sockets.add(13).unwrap();
        assert_eq!(tcp_sock.0, 0);
        assert_eq!(tcp_sockets.get(tcp_sock).unwrap().0.v, 0);
        let udp_sock = udp_sockets.add(42).unwrap();
        assert_eq!(udp_sock.0, 0);
        assert_eq!(udp_sockets.get(udp_sock).unwrap().0.v, 7);
    }

    #[test]
    fn test_socket_placeholder() {
        // Verify a new socket is created for the requested entry
        let mut socks = SockHolder::<7, 0>::default();
        let opt_hdl = socks.put(Handle(1), 13);

        assert_eq!(opt_hdl.unwrap().0, 1);
        assert_eq!(socks.get(Handle(0)), None);
        assert_eq!(socks.get(Handle(2)), None);

        // Verify if the socket index is already taken
        let mut socks = SockHolder::<7, 0>::default();
        let handle = socks.add(13);
        let opt_hdl = socks.put(handle.unwrap(), 13);

        assert_eq!(opt_hdl, None);

        // Verify no space is available for new entries
        let mut socks = SockHolder::<1, 0>::default();
        let _handle = socks.add(13);
        let opt_hdl = socks.put(Handle(1), 13);

        assert_eq!(opt_hdl, None);
    }
}
