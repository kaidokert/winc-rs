use crate::manager::SOCKET_BUFFER_MAX_LENGTH;
use crate::manager::{EventListener, Manager};
use crate::socket::Socket;
use crate::transfer::Xfer;

use crate::Ipv4AddrFormatWrapper;

use crate::manager::SocketError;

use crate::{debug, error};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Handle(pub u8);

mod dns;
mod stack_error;
mod tcp_stack;
mod udp_stack;
pub use stack_error::StackError;

#[derive(PartialEq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ClientSocketOp {
    None,
    New,
    Connect,
    Send,
    SendTo,
    Recv,
    RecvFrom,
    Bind,
}

#[derive(PartialEq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum GlobalOp {
    GetHostByName,
    #[allow(dead_code)] // todo: we'll add this later
    Ping,
}

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
    pub fn get(&mut self, handle: Handle) -> Option<&mut (Socket, ClientSocketOp)> {
        self.sockets[handle.0 as usize].as_mut()
    }
}

struct SocketCallbacks {
    // #define TCP_SOCK_MAX										(7)
    // indexes 0-6
    tcp_sockets: SockHolder<7, 0>,
    // #define UDP_SOCK_MAX										4
    udp_sockets: SockHolder<4, 7>,
    recv_buffer: [u8; SOCKET_BUFFER_MAX_LENGTH],
    recv_len: usize,
    // Todo: move this into socket
    last_error: crate::manager::SocketError,
    last_recv_addr: Option<core::net::SocketAddrV4>,
    global_op: Option<GlobalOp>,
}

impl SocketCallbacks {
    pub fn new() -> Self {
        Self {
            tcp_sockets: SockHolder::new(),
            udp_sockets: SockHolder::new(),
            recv_buffer: [0; SOCKET_BUFFER_MAX_LENGTH],
            recv_len: 0,
            last_error: crate::manager::SocketError::NoError,
            last_recv_addr: None,
            global_op: None,
        }
    }
    fn resolve(&mut self, socket: Socket) -> Option<&mut (Socket, ClientSocketOp)> {
        if socket.v < 7 {
            debug!("resolving tcp: {:?}", socket.v);
            self.tcp_sockets.get(Handle(socket.v))
        } else {
            debug!("resolving udp: {:?}", socket.v);
            self.udp_sockets.get(Handle(socket.v - 7))
        }
    }
}

impl EventListener for SocketCallbacks {
    fn on_dhcp(&mut self, conf: crate::manager::IPConf) {
        debug!("on_dhcp: IP config: {}", conf);
    }
    fn on_connect(&mut self, socket: Socket, err: crate::manager::SocketError) {
        debug!("on_connect: socket {:?}", socket);

        if let Some((s, op)) = self.resolve(socket) {
            if *op == ClientSocketOp::Connect {
                debug!("on_connect: socket:{:?} error:{:?}", s, err);
                *op = ClientSocketOp::None;
                self.last_error = err;
            } else {
                error!(
                    "UNKNOWN STATE on_connect (x): socket:{:?} error:{:?} state:{:?}",
                    s, err, *op
                );
            }
        } else {
            error!(
                "on_connect (x): COULD NOT FIND SOCKET socket:{:?} error:{:?}",
                socket, err
            );
        }
    }
    fn on_send_to(&mut self, socket: Socket, len: i16) {
        debug!("on_send_to: socket:{:?} length:{:?}", socket, len);
        if let Some((s, op)) = self.resolve(socket) {
            if *op == ClientSocketOp::SendTo {
                debug!("on_send_to: socket:{:?} length:{:?}", socket, len);
                *op = ClientSocketOp::None;
            } else {
                error!(
                    "UNKNOWN STATE on_send_to (x): socket:{:?} len:{:?} state:{:?}",
                    s, len, *op
                );
            }
        } else {
            error!(
                "UNKNOWN STATE on_send_to (x): socket:{:?} len:{:?}",
                socket, len
            );
        }
    }
    fn on_send(&mut self, socket: Socket, len: i16) {
        debug!("on_send: socket {:?}", socket);

        if let Some((s, op)) = self.resolve(socket) {
            if *op == ClientSocketOp::Send {
                debug!("on_send: socket:{:?} length:{:?}", socket, len);
                *op = ClientSocketOp::None;
            } else {
                error!(
                    "UNKNOWN STATE on_send (x): socket:{:?} len:{:?} state:{:?}",
                    s, len, *op
                );
            }
        } else {
            error!(
                "on_send (x): COULD NOT FIND SOCKET socket:{:?} len:{:?}",
                socket, len
            );
        }
    }
    fn on_recv(
        &mut self,
        socket: Socket,
        address: core::net::SocketAddrV4,
        data: &[u8],
        err: crate::manager::SocketError,
    ) {
        debug!("on_recv: socket {:?}", socket);
        let mut found = false;
        if let Some((s, op)) = self.resolve(socket) {
            if *op == ClientSocketOp::Recv {
                debug!(
                    "on_recv: socket:{:?} address:{:?} data:{:?} len:{:?} error:{:?}",
                    s,
                    Ipv4AddrFormatWrapper::new(address.ip()),
                    data,
                    data.len(),
                    err
                );
                #[cfg(feature = "defmt")]
                {
                    defmt::info!(
                        "on_recv: socket:{:?} address:{:?} data:{=[u8]:#04x} len:{:?} error:{:?}",
                        s,
                        Ipv4AddrFormatWrapper::new(address.ip()),
                        data,
                        data.len(),
                        err
                    );
                }
                *op = ClientSocketOp::None;
                found = true;
            } else {
                error!(
                    "UNKNOWN on_recv: socket:{:?} address:{:?} port:{:?} data:{:?} len:{:?} error:{:?}",
                    socket,
                    Ipv4AddrFormatWrapper::new(address.ip()),
                    address.port(),
                    data,
                    data.len(),
                    err
                );
            }
        } else {
            error!(
                "UNKNOWN on_recv: socket:{:?} address:{:?} port:{:?} data:{:?} error:{:?}",
                socket,
                Ipv4AddrFormatWrapper::new(address.ip()),
                address.port(),
                data,
                err
            );
        }
        if found {
            self.recv_buffer[..data.len()].copy_from_slice(data);
            self.recv_len = data.len();
            self.last_error = err;
        }
    }
    fn on_recvfrom(
        &mut self,
        socket: Socket,
        address: core::net::SocketAddrV4,
        data: &[u8],
        err: crate::manager::SocketError,
    ) {
        debug!("on_recvfrom: socket {:?}", socket);
        let mut found = false;
        if let Some((s, op)) = self.resolve(socket) {
            if *op == ClientSocketOp::RecvFrom {
                debug!(
                    "on_recvfrom: socket:{:?} address:{:?} data:{:?} error:{:?}",
                    s,
                    Ipv4AddrFormatWrapper::new(address.ip()),
                    data,
                    err
                );
                *op = ClientSocketOp::None;
                self.last_error = err;
                found = true;
            } else {
                error!(
                    "UNKNOWN on_recvfrom: socket:{:?} address:{:?} data:{:?} error:{:?}",
                    socket,
                    Ipv4AddrFormatWrapper::new(address.ip()),
                    data,
                    err
                );
            }
        } else {
            error!(
                "UNKNOWN on_recvfrom: socket:{:?} address:{:?} data:{:?} error:{:?}",
                socket,
                Ipv4AddrFormatWrapper::new(address.ip()),
                data,
                err
            );
        }
        if found {
            self.recv_buffer[..data.len()].copy_from_slice(data);
            self.recv_len = data.len();
            self.last_error = err;
            self.last_recv_addr = Some(address);
        }
    }
    fn on_system_time(&mut self, year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) {
        debug!(
            "on_system_time: {}-{:02}-{:02} {:02}:{:02}:{:02}",
            year, month, day, hour, minute, second
        );
    }
    fn on_resolve(&mut self, ip: core::net::Ipv4Addr, host: &str) {
        debug!(
            "on_resolve: ip:{:?} host:{:?}",
            Ipv4AddrFormatWrapper::new(&ip),
            host
        );

        let Some(op) = self.global_op else {
            error!("UNKNOWN on_resolve: host:{:?}", host);
            return;
        };

        match op {
            GlobalOp::GetHostByName => {
                debug!(
                    "on_resolve: ip:{:?} host:{:?}",
                    Ipv4AddrFormatWrapper::new(&ip),
                    host
                );
                self.last_recv_addr = Some(core::net::SocketAddrV4::new(ip, 0));
                self.global_op = None; // ends polling
            }
            _ => {
                error!("UNKNOWN on_resolve: host: {} state:{:?}", host, op);
            }
        }
    }
    fn on_bind(&mut self, sock: Socket, err: crate::manager::SocketError) {
        debug!("on_bind: socket {:?}", sock);
        if let Some((s, op)) = self.resolve(sock) {
            if *op == ClientSocketOp::Bind {
                *op = ClientSocketOp::None;
                self.last_error = err;
            } else {
                error!(
                    "UNKNOWN on_bind: socket:{:?} error:{:?} state:{:?}",
                    s, err, *op
                );
            }
        }
    }
}

pub struct WincClient<'a, X: Xfer, E: EventListener> {
    manager: Manager<X, E>,
    delay: &'a mut dyn FnMut(u32) -> (),
    recv_timeout: u32,
    poll_loop_delay: u32,
    callbacks: SocketCallbacks,
    next_session_id: u16,
    // TODO: Lets change that per socket
    last_send_addr: Option<core::net::SocketAddrV4>,
}

pub enum GenResult {
    Ip(core::net::Ipv4Addr),
    Len(usize),
}

impl<'a, X: Xfer, E: EventListener> WincClient<'a, X, E> {
    const BIND_TIMEOUT: u32 = 100;
    const SEND_TIMEOUT: u32 = 1000;
    const RECV_TIMEOUT: u32 = 1000;
    const CONNECT_TIMEOUT: u32 = 1000;
    const POLL_LOOP_DELAY: u32 = 100;
    const DNS_TIMEOUT: u32 = 1000;
    pub fn new(manager: Manager<X, E>, delay: &'a mut impl FnMut(u32)) -> Self {
        Self {
            manager,
            callbacks: SocketCallbacks::new(),
            delay,
            recv_timeout: Self::RECV_TIMEOUT,
            poll_loop_delay: Self::POLL_LOOP_DELAY,
            next_session_id: 0,
            last_send_addr: None,
        }
    }
    fn get_next_session_id(&mut self) -> u16 {
        let ret = self.next_session_id;
        self.next_session_id += 1;
        ret
    }
    pub fn dispatch_events(&mut self) -> Result<(), StackError> {
        self.manager
            .dispatch_events_new(&mut self.callbacks)
            .map_err(|some_err| StackError::DispatchError(some_err))
    }
    fn wait_with_timeout<F, T>(
        &mut self,
        timeout: u32,
        mut check_complete: F,
    ) -> Result<T, StackError>
    where
        F: FnMut(&mut Self) -> Option<Result<T, StackError>>,
    {
        self.dispatch_events()?;
        let mut timeout = timeout as i32;

        loop {
            if timeout <= 0 {
                return Err(StackError::GeneralTimeout);
            }

            if let Some(result) = check_complete(self) {
                return result;
            }

            (self.delay)(self.poll_loop_delay);
            self.dispatch_events()?;
            timeout -= self.poll_loop_delay as i32;
        }
    }

    fn wait_for_gen_ack(
        &mut self,
        expect_op: GlobalOp,
        timeout: u32,
    ) -> Result<GenResult, StackError> {
        debug!("===>Waiting for gen ack for {:?}", expect_op);

        self.wait_with_timeout(timeout, |client| {
            if client.callbacks.global_op == None {
                debug!("<===Ack received {:?}", expect_op);

                if let Some(addr) = client.callbacks.last_recv_addr {
                    return Some(Ok(GenResult::Ip(*addr.ip())));
                }

                if client.callbacks.last_error != SocketError::NoError {
                    return Some(Err(StackError::OpFailed(client.callbacks.last_error)));
                }

                return Some(Err(StackError::GlobalOpFailed));
            }
            None
        })
        .or_else(|e| {
            if matches!(e, StackError::GeneralTimeout) {
                match expect_op {
                    GlobalOp::GetHostByName => Err(StackError::DnsTimeout),
                    _ => Err(StackError::GeneralTimeout),
                }
            } else {
                Err(e)
            }
        })
    }

    fn wait_for_op_ack(
        &mut self,
        handle: Handle,
        expect_op: ClientSocketOp,
        timeout: u32,
        tcp: bool,
    ) -> Result<GenResult, StackError> {
        debug!("===>Waiting for op ack for {:?}", expect_op);

        self.wait_with_timeout(timeout, |client| {
            let (_sock, op) = match tcp {
                true => client.callbacks.tcp_sockets.get(handle).unwrap(),
                false => client.callbacks.udp_sockets.get(handle).unwrap(),
            };

            if *op == ClientSocketOp::None {
                debug!(
                    "<===Ack received for {:?}, recv_len:{:?}",
                    expect_op, client.callbacks.recv_len
                );

                if client.callbacks.last_error != SocketError::NoError {
                    return Some(Err(StackError::OpFailed(client.callbacks.last_error)));
                }

                return Some(Ok(GenResult::Len(client.callbacks.recv_len)));
            }
            None
        })
        .or_else(|e| {
            if matches!(e, StackError::GeneralTimeout) {
                match expect_op {
                    ClientSocketOp::Connect => Err(StackError::ConnectTimeout),
                    ClientSocketOp::Send => Err(StackError::SendTimeout),
                    ClientSocketOp::Recv => Err(StackError::RecvTimeout),
                    _ => Err(StackError::GeneralTimeout),
                }
            } else {
                Err(e)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_winc_client() {}

    #[test]
    fn test_fa_client() {}

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
}
