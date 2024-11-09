use crate::manager::{EventListener, Manager};
use crate::transfer::Xfer;
use crate::Socket;
use core::marker::PhantomData;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Handle(pub u8);

#[derive(PartialEq, Clone, Copy)]
#[cfg_attr(not(feature = "std"), derive(defmt::Format))]
pub enum ClientSocketOp {
    None,
    New,
    Connect,
    Send,
    Recv,
    Close,
}

pub struct SockHolder<const N: usize, const BASE: usize> {
    sockets: [Option<(Socket,ClientSocketOp)>; N],
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
    pub fn add(&mut self, session_id: u16) -> Result<Handle, i32> {
        if self.len() >= N {
            return Err(-1);
        }
        for (index, element) in self.sockets.iter_mut().enumerate() {
            if element.is_none() {
                let ns = Socket::new((BASE + index) as u8, session_id);
                element.replace((ns, ClientSocketOp::New));
                return Ok(Handle(index as u8));
            }
        }
        Err(-1)
    }
    pub fn remove(&mut self, handle: Handle) {
        self.sockets[handle.0 as usize] = None;
    }
    pub fn get(&mut self, handle: Handle) -> Option<&mut (Socket, ClientSocketOp)> {
        self.sockets[handle.0 as usize].as_mut()
    }
}

pub struct Listener {}
impl EventListener for Listener {
    fn on_rssi(&mut self, _rssi: i8) {}
}

pub struct WincClient<X: Xfer> {
    pub(super) _tcp_sockets: SockHolder<7, 0>,
    pub(super) udp_sockets: SockHolder<3, 7>,
    next_session_id: u16,
    phantom: PhantomData<X>,
    pub(super) manager: Option<Manager<X, Listener>>,
}

impl<X: Xfer> WincClient<X> {
    pub fn new() -> Self {
        Self {
            _tcp_sockets: SockHolder::new(),
            udp_sockets: SockHolder::new(),
            next_session_id: 1,
            phantom: PhantomData,
            manager: None,
        }
    }
    pub fn from_xfer(xfer: X) -> Self {
        let mut client = WincClient::<X>::new();
        let mgr = Manager::from_xfer(xfer, Listener {});
        client.manager.replace(mgr);
        client
    }
    pub(super) fn get_next_session_id(&mut self) -> u16 {
        let ret = self.next_session_id;
        self.next_session_id += 1;
        ret
    }
    pub fn spin(&mut self) -> Result<(), u32> {
        Ok(())
    }
}

pub struct ConnectionOptions {}

impl<X: Xfer> WincClient<X> {
    pub fn connect(&mut self, _options: &ConnectionOptions) {
        todo!()
    }
    pub fn scan(&mut self) {
        todo!()
    }
}

impl<X: Xfer> Default for WincClient<X> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer::PrefixXfer;

    #[test]
    fn test_winc_client() {
        let mut client = WincClient::<PrefixXfer<&mut [u8]>>::new();
    }

    #[test]
    fn test_fa_client() {
        let mut fa_client = WincClient::<PrefixXfer<&mut [u8]>>::new();
        assert_eq!(fa_client._tcp_sockets.len(), 0);
        assert_eq!(fa_client.udp_sockets.len(), 0);
        assert_eq!(fa_client._tcp_sockets.add(0).unwrap().0, 0);
        assert_eq!(fa_client._tcp_sockets.add(1).unwrap().0, 1);
        assert_eq!(fa_client.udp_sockets.add(2).unwrap().0, 0);
        assert_eq!(fa_client.udp_sockets.add(3).unwrap().0, 1);
        assert_eq!(fa_client.udp_sockets.add(4).unwrap().0, 2);
        assert_eq!(fa_client.udp_sockets.add(5), Err(-1));
        assert_eq!(fa_client._tcp_sockets.len(), 2);
        assert_eq!(fa_client.udp_sockets.len(), 3);
    }

    #[test]
    fn test_containers() {
        let mut socks = SockHolder::<2, 1>::new();
        let handle = socks.add(0).unwrap();
        let s = socks.get(handle).unwrap();
    }
}
