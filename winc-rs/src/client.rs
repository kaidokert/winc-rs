use core::net::Ipv4Addr;

use crate::manager::{
    ConnectionInfo, PingError, ScanResult, WifiConnError, WifiConnState, SOCKET_BUFFER_MAX_LENGTH,
};
use crate::manager::{EventListener, Manager};
use crate::socket::Socket;
use crate::transfer::Xfer;

use crate::Ipv4AddrFormatWrapper;

use crate::manager::SocketError;

use crate::{debug, error, info};

/// Opaque handle to a socket. Returned by socket APIs
#[derive(Clone, Copy, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Handle(u8);

mod dns;
mod stack_error;
mod tcp_stack;
mod udp_stack;
mod wifi_module;
pub use stack_error::StackError;
use wifi_module::WifiModuleState;

#[derive(PartialEq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ClientSocketOp {
    None,
    New,
    Connect,
    Send(i16),
    SendTo(i16),
    Recv,
    RecvFrom,
    Bind,
    Listen,
    Accept,
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

// TODO: This should be exposed to user
#[allow(dead_code)]
struct SystemTime {
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
}

/// Ping operation results
#[derive(Debug)]
pub struct PingResult {
    pub ip: Ipv4Addr,
    pub rtt: u32,
    pub num_successful: u16,
    pub num_failed: u16,
    pub error: PingError,
}

#[cfg(feature = "defmt")]
impl defmt::Format for PingResult {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(
            f,
            "ip: {}, rtt: {}, num_successful: {}, num_failed: {}, error: {}",
            Ipv4AddrFormatWrapper::new(&self.ip),
            self.rtt,
            self.num_successful,
            self.num_failed,
            self.error
        );
    }
}

struct ConnectionState {
    conn_state: WifiConnState,
    conn_error: Option<WifiConnError>,
    ip_conf: Option<crate::manager::IPConf>,
    system_time: Option<SystemTime>,
    ip_conflict: Option<Ipv4Addr>,
    scan_number_aps: Option<Option<u8>>,
    scan_results: Option<Option<ScanResult>>,
    conn_info: Option<Option<ConnectionInfo>>,
    rssi_level: Option<Option<i8>>,
    ping_result: Option<Option<PingResult>>,
}

struct SocketCallbacks {
    // #define TCP_SOCK_MAX										(7)
    // indexes 0-6
    tcp_sockets: SockHolder<7, 0>,
    // #define UDP_SOCK_MAX										4
    udp_sockets: SockHolder<4, 7>,
    recv_buffer: [u8; SOCKET_BUFFER_MAX_LENGTH],

    // All this should be moved into an enum rather, these are response
    // callbacks, mutually exclusive
    recv_len: usize,
    // Todo: Maybe per socket ?
    last_error: crate::manager::SocketError,
    last_recv_addr: Option<core::net::SocketAddrV4>,
    last_accepted_socket: Option<Socket>,
    global_op: Option<GlobalOp>,
    connection_state: ConnectionState,
    state: wifi_module::WifiModuleState,
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
            last_accepted_socket: None,
            global_op: None,
            connection_state: ConnectionState::new(),
            state: wifi_module::WifiModuleState::Reset,
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

impl ConnectionState {
    fn new() -> Self {
        Self {
            conn_state: WifiConnState::Disconnected,
            conn_error: None,
            ip_conf: None,
            system_time: None,
            rssi_level: None,
            ip_conflict: None,
            conn_info: None,
            scan_number_aps: None,
            scan_results: None,
            ping_result: None,
        }
    }
}

impl EventListener for SocketCallbacks {
    fn on_rssi(&mut self, level: i8) {
        info!("client: Got RSSI:{}", level);
        self.connection_state.rssi_level = Some(Some(level));
    }

    fn on_resolve(&mut self, ip: core::net::Ipv4Addr, host: &str) {
        debug!(
            "on_resolve: ip:{:?} host:{:?}",
            Ipv4AddrFormatWrapper::new(&ip),
            host
        );
        match self.global_op {
            Some(GlobalOp::GetHostByName) => {
                debug!(
                    "on_resolve: ip:{:?} host:{:?}",
                    Ipv4AddrFormatWrapper::new(&ip),
                    host
                );
                self.last_recv_addr = Some(core::net::SocketAddrV4::new(ip, 0));
                self.global_op = None; // ends polling
            }
            Some(op) => {
                error!("UNKNOWN on_resolve: host: {} state:{:?}", host, op);
            }
            _ => {
                error!("UNKNOWN on_resolve: host: {}", host);
            }
        }
    }

    fn on_default_connect(&mut self, connected: bool) {
        debug!("client: got connected {}", connected)
    }
    fn on_dhcp(&mut self, conf: crate::manager::IPConf) {
        debug!("client: on_dhcp: IP config: {}", conf);
        self.connection_state.ip_conf = Some(conf);
    }
    fn on_connstate_changed(&mut self, state: WifiConnState, err: WifiConnError) {
        debug!("client: Connection state changed: {:?} {:?}", state, err);
        self.connection_state.conn_state = state;
        self.connection_state.conn_error = Some(err);
        match self.state {
            WifiModuleState::ConnectingToAp => match self.connection_state.conn_state {
                WifiConnState::Connected => {
                    self.state = WifiModuleState::ConnectedToAp;
                }
                _ => {
                    self.state = WifiModuleState::ConnectionFailed;
                    debug!(
                        "on_connstate_changed FAILED: {:?} {:?}",
                        self.connection_state.conn_state, self.connection_state.conn_error
                    );
                }
            },
            _ => {
                error!(
                    "UNKNOWN STATE on_connstate_changed: {:?} {:?}",
                    self.connection_state.conn_state, self.connection_state.conn_error
                );
            }
        }
    }

    fn on_connection_info(&mut self, info: ConnectionInfo) {
        debug!("client: conninfo, state:{}", info);
        self.connection_state.conn_info = Some(Some(info));
    }
    fn on_system_time(&mut self, year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) {
        debug!(
            "client: on_system_time: {}-{:02}-{:02} {:02}:{:02}:{:02}",
            year, month, day, hour, minute, second
        );
        self.connection_state.system_time = Some(SystemTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
        });
    }
    fn on_ip_conflict(&mut self, ip: Ipv4Addr) {
        info!(
            "client: on_ip_conflict: {:?}",
            Ipv4AddrFormatWrapper::new(&ip)
        );
        self.connection_state.ip_conflict = Some(ip);
    }

    fn on_scan_result(&mut self, result: ScanResult) {
        debug!("Scanresult {}", result);
        self.connection_state.scan_results = Some(Some(result));
    }
    fn on_scan_done(&mut self, num_aps: u8, err: WifiConnError) {
        debug!("Scan done, aps:{} error:{}", num_aps, err);
        if err != WifiConnError::Unhandled {
            self.connection_state.conn_error = Some(err);
        }
        self.connection_state.scan_number_aps = Some(Some(num_aps));
    }
    fn on_ping(
        &mut self,
        ip: Ipv4Addr,
        token: u32,
        rtt: u32,
        num_successful: u16,
        num_failed: u16,
        error: PingError,
    ) {
        let ping_result = PingResult {
            ip,
            rtt,
            num_successful,
            num_failed,
            error,
        };
        debug!("client: on_ping: {:?} token:# {}", ping_result, token);
        self.connection_state.ping_result = Some(Some(ping_result));
    }

    // todo: Consolidate the error cases to match statements below
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
            match op {
                ClientSocketOp::SendTo(req_len) => {
                    if len >= *req_len {
                        debug!("FIN: on_send_to: socket:{:?} length:{:?}", socket, len);
                        *op = ClientSocketOp::None;
                    } else {
                        debug!("CONT: on_send_to: socket:{:?} length:{:?}", socket, len);
                        *req_len -= len;
                    }
                }
                _ => {
                    error!(
                        "UNKNOWN STATE on_send_to (x): socket:{:?} len:{:?} state:{:?}",
                        s, len, *op
                    );
                }
            }
        } else {
            error!(
                "UNKNOWN STATE on_send_to (x): socket:{:?} len:{:?}",
                socket, len
            );
        }
    }
    fn on_send(&mut self, socket: Socket, len: i16) {
        debug!("on_send: socket {:?} len:{}", socket, len);

        if let Some((s, op)) = self.resolve(socket) {
            match op {
                ClientSocketOp::Send(req_len) => {
                    if len >= *req_len {
                        debug!("FIN: on_send: socket:{:?} length:{:?}", socket, len);
                        *op = ClientSocketOp::None;
                    } else {
                        debug!("CONT: on_send: socket:{:?} length:{:?}", socket, len);
                        *req_len -= len;
                    }
                }
                _ => {
                    error!(
                        "UNKNOWN STATE on_send (x): socket:{:?} len:{:?} state:{:?}",
                        s, len, *op
                    );
                }
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
    fn on_listen(&mut self, sock: Socket, err: crate::manager::SocketError) {
        debug!("on_listen: socket {:?}", sock);
        if let Some((s, op)) = self.resolve(sock) {
            if *op == ClientSocketOp::Listen {
                *op = ClientSocketOp::None;
                self.last_error = err;
            } else {
                error!(
                    "UNKNOWN on_listen: socket:{:?} error:{:?} state:{:?}",
                    s, err, *op
                );
            }
        }
    }

    // This is different, no error being passed
    fn on_accept(
        &mut self,
        address: core::net::SocketAddrV4,
        listen_socket: Socket,
        accepted_socket: Socket,
        _data_offset: u16,
    ) {
        debug!(
            "on_accept: address:{:?} port:{:?} listen_socket:{:?} accepted_socket:{:?}",
            Ipv4AddrFormatWrapper::new(address.ip()),
            address.port(),
            listen_socket,
            accepted_socket
        );

        if let Some((_s, op)) = self.resolve(listen_socket) {
            if *op == ClientSocketOp::Accept {
                *op = ClientSocketOp::None;
                self.last_error = SocketError::NoError;
                self.last_recv_addr = Some(address);
                self.last_accepted_socket = Some(accepted_socket);
            } else {
                error!(
                    "Socket was NOT in accept : address:{:?} port:{:?} listen_socket:{:?} accepted_socket:{:?}
                    actual state:{:?}",
                Ipv4AddrFormatWrapper::new(address.ip()),
                address.port(),
                listen_socket,
                accepted_socket,
                *op);
            }
        } else {
            error!(
                "UNKNOWN socket on_accept: address:{:?} port:{:?} listen_socket:{:?} accepted_socket:{:?}",
                Ipv4AddrFormatWrapper::new(address.ip()),
                address.port(),
                listen_socket,
                accepted_socket
            );
        };
    }
}

pub enum GenResult {
    Ip(core::net::Ipv4Addr),
    Len(usize),
    Accept(core::net::SocketAddrV4, Socket),
}

/// Client for the WincWifi chip.
///
/// This manages the state of the chip and
/// network connections
pub struct WincClient<'a, X: Xfer> {
    manager: Manager<X>,
    delay: &'a mut dyn FnMut(u32),
    recv_timeout: u32,
    poll_loop_delay: u32,
    callbacks: SocketCallbacks,
    next_session_id: u16,
    // TODO: Lets change that per socket
    last_send_addr: Option<core::net::SocketAddrV4>,
    boot: Option<crate::manager::BootState>,
    operation_countdown: u32,
    #[cfg(test)]
    debug_callback: Option<&'a mut dyn FnMut(&mut SocketCallbacks)>,
}

impl<'a, X: Xfer> WincClient<'a, X> {
    // Max send frame length
    const MAX_SEND_LENGTH: usize = 1400;

    const TCP_SOCKET_BACKLOG: u8 = 4;
    const LISTEN_TIMEOUT: u32 = 100;
    const ACCEPT_TIMEOUT: u32 = 100;
    const BIND_TIMEOUT: u32 = 100;
    const SEND_TIMEOUT: u32 = 1000;
    const RECV_TIMEOUT: u32 = 1000;
    const CONNECT_TIMEOUT: u32 = 1000;
    const DNS_TIMEOUT: u32 = 1000;
    const POLL_LOOP_DELAY: u32 = 10;
    /// Create a new WincClient..
    ///
    /// # Arguments
    ///
    /// * `transfer` - The transfer implementation to use for client,
    ///             typically a struct wrapping SPI communication.
    /// * `delay` - A delay function. Currently required - a closure
    ///             that takes millisconds as an arg.
    ///
    ///  See [Xfer] for details how to implement a transfer struct.
    pub fn new(transfer: X, delay: &'a mut impl FnMut(u32)) -> Self {
        let manager = Manager::from_xfer(transfer);
        Self::new_internal(manager, delay)
    }
    fn new_internal(manager: Manager<X>, delay: &'a mut impl FnMut(u32)) -> Self {
        Self {
            manager,
            callbacks: SocketCallbacks::new(),
            delay,
            recv_timeout: Self::RECV_TIMEOUT,
            poll_loop_delay: Self::POLL_LOOP_DELAY,
            next_session_id: 0,
            last_send_addr: None,
            boot: None,
            operation_countdown: 0,
            #[cfg(test)]
            debug_callback: None,
        }
    }
    fn get_next_session_id(&mut self) -> u16 {
        let ret = self.next_session_id;
        self.next_session_id += 1;
        ret
    }
    fn dispatch_events(&mut self) -> Result<(), StackError> {
        #[cfg(test)]
        if let Some(callback) = &mut self.debug_callback {
            callback(&mut self.callbacks);
        }
        self.manager
            .dispatch_events_new(&mut self.callbacks)
            .map_err(StackError::DispatchError)
    }
    fn wait_with_timeout<F, T>(
        &mut self,
        timeout: u32,
        mut check_complete: F,
    ) -> Result<T, StackError>
    where
        F: FnMut(&mut Self, u32) -> Option<Result<T, StackError>>,
    {
        self.dispatch_events()?;
        let mut timeout = timeout as i32;
        let mut elapsed = 0;

        loop {
            if timeout <= 0 {
                return Err(StackError::GeneralTimeout);
            }

            if let Some(result) = check_complete(self, elapsed) {
                return result;
            }

            (self.delay)(self.poll_loop_delay);
            self.dispatch_events()?;
            timeout -= self.poll_loop_delay as i32;
            elapsed += self.poll_loop_delay;
        }
    }

    fn wait_for_gen_ack(
        &mut self,
        expect_op: GlobalOp,
        timeout: u32,
    ) -> Result<GenResult, StackError> {
        // Lets clear state
        self.callbacks.last_recv_addr = None;
        self.callbacks.last_error = SocketError::NoError;

        debug!("===>Waiting for gen ack for {:?}", expect_op);

        self.wait_with_timeout(timeout, |client, elapsed| {
            if client.callbacks.global_op.is_none() {
                debug!("<===Ack received {:?} elapsed:{}ms", expect_op, elapsed);

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
        .map_err(|e| {
            if matches!(e, StackError::GeneralTimeout) {
                match expect_op {
                    GlobalOp::GetHostByName => StackError::DnsTimeout,
                    _ => StackError::GeneralTimeout,
                }
            } else {
                e
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
        self.callbacks.last_recv_addr = None;
        self.callbacks.last_error = SocketError::NoError;

        debug!("===>Waiting for op ack for {:?}", expect_op);

        self.wait_with_timeout(timeout, |client, elapsed| {
            let (_sock, op) = match tcp {
                true => client.callbacks.tcp_sockets.get(handle).unwrap(),
                false => client.callbacks.udp_sockets.get(handle).unwrap(),
            };

            if *op == ClientSocketOp::None {
                debug!(
                    "<===Ack received for {:?}, recv_len:{:?}, elapsed:{}ms",
                    expect_op, client.callbacks.recv_len, elapsed
                );

                if let Some(accepted_socket) = client.callbacks.last_accepted_socket.take() {
                    return Some(Ok(GenResult::Accept(
                        client.callbacks.last_recv_addr.unwrap(),
                        accepted_socket,
                    )));
                }

                if client.callbacks.last_error != SocketError::NoError {
                    return Some(Err(StackError::OpFailed(client.callbacks.last_error)));
                }

                return Some(Ok(GenResult::Len(client.callbacks.recv_len)));
            }
            None
        })
        .map_err(|e| {
            if matches!(e, StackError::GeneralTimeout) {
                match expect_op {
                    ClientSocketOp::Connect => StackError::ConnectTimeout,
                    ClientSocketOp::Send(_) => StackError::SendTimeout,
                    ClientSocketOp::Recv => StackError::RecvTimeout,
                    _ => StackError::GeneralTimeout,
                }
            } else {
                e
            }
        })
    }
}

#[cfg(test)]
mod test_shared {
    use super::*;

    pub(crate) struct MockTransfer {}

    impl Default for MockTransfer {
        fn default() -> Self {
            Self {}
        }
    }

    impl Xfer for MockTransfer {
        fn recv(&mut self, _: &mut [u8]) -> Result<(), crate::errors::Error> {
            Ok(())
        }
        fn send(&mut self, _: &[u8]) -> Result<(), crate::errors::Error> {
            Ok(())
        }
    }

    pub(crate) fn make_test_client(delay: &mut impl FnMut(u32)) -> WincClient<MockTransfer> {
        let mut client = WincClient::new(MockTransfer::default(), delay);
        client.manager.set_unit_test_mode();
        client
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
