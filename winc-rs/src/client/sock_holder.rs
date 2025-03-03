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
    pub fn add(&mut self, session_id: u16) -> Option<Handle> {
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
    pub fn remove(&mut self, handle: Handle) {
        self.sockets[handle.0 as usize] = None;
    }
    pub fn put(&mut self, handle: Handle, session_id: u16) -> Option<Handle> {
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

    pub fn get(&mut self, handle: Handle) -> Option<&mut (Socket, ClientSocketOp)> {
        self.sockets[handle.0 as usize].as_mut()
    }
}
