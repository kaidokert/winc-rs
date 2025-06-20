use core::net::Ipv4Addr;

#[cfg(feature = "wep")]
use crate::manager::WepKey;
#[cfg(feature = "wep")]
use crate::manager::WepKeyIndex;
use crate::manager::{
    Credentials, EventListener, ProvisioningInfo, SocketError, Ssid, WifiConnError, WifiConnState,
    WpaKey, PRNG_DATA_LENGTH,
};
use crate::{debug, error, info};
use crate::{AuthType, ConnectionInfo};

use crate::socket::Socket;

use crate::Ipv4AddrFormatWrapper;

use super::SockHolder;
use crate::manager::{PingError, ScanResult, SOCKET_BUFFER_MAX_LENGTH};

use crate::stack::sock_holder::SocketStore;

/// Opaque handle to a socket. Returned by socket APIs
#[derive(Clone, Copy, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Handle(pub u8);

#[derive(Debug, PartialEq, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(crate) enum WifiModuleState {
    Reset,
    Starting,
    Unconnected,
    ConnectingToAp,
    ConnectedToAp,
    ConnectionFailed,
    Disconnecting,
    Provisioning,
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

// TODO: This should be exposed to user
#[allow(dead_code)]
pub struct SystemTime {
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
}

pub(crate) struct ConnectionState {
    conn_state: WifiConnState,
    pub conn_error: Option<WifiConnError>,
    pub ip_conf: Option<crate::manager::IPConf>,
    system_time: Option<SystemTime>,
    ip_conflict: Option<Ipv4Addr>,
    pub scan_number_aps: Option<Option<u8>>,
    pub scan_results: Option<Option<ScanResult>>,
    pub conn_info: Option<Option<ConnectionInfo>>,
    pub rssi_level: Option<Option<i8>>,
    pub ping_result: Option<Option<PingResult>>,
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

pub(crate) const NUM_TCP_SOCKETS: usize = 7;
pub(crate) const MAX_UDP_SOCKETS: usize = 4;

pub(crate) struct SocketCallbacks {
    // #define TCP_SOCK_MAX										(7)
    // indexes 0-6
    pub tcp_sockets: SockHolder<NUM_TCP_SOCKETS, 0>,
    // #define UDP_SOCK_MAX										4
    pub udp_sockets: SockHolder<MAX_UDP_SOCKETS, NUM_TCP_SOCKETS>,
    pub listening_sockets: [bool; NUM_TCP_SOCKETS],
    pub accept_backlog: [Option<(Socket, core::net::SocketAddrV4)>; NUM_TCP_SOCKETS],
    // Needed to keep track of connect() and recvfrom address
    pub udp_socket_connect_addr: [Option<core::net::SocketAddrV4>; MAX_UDP_SOCKETS],
    pub recv_buffer: [u8; SOCKET_BUFFER_MAX_LENGTH],

    // This is global
    pub dns_resolved_addr: Option<Option<core::net::Ipv4Addr>>,
    pub connection_state: ConnectionState,
    pub state: WifiModuleState,
    // Random Number Generator
    pub prng: Option<Option<Prng>>,
    pub provisioning_info: Option<Option<ProvisioningInfo>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(crate) struct ConnectResult {
    pub error: SocketError,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(crate) struct SendRequest {
    pub offset: usize,
    pub grand_total_sent: i16,
    pub total_sent: i16,
    pub remaining: i16,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RecvResult {
    pub recv_len: usize,
    pub from_addr: core::net::SocketAddrV4,
    pub error: SocketError,
    pub return_offset: usize, // Track how much data has been returned to caller
}
#[cfg(feature = "defmt")]
impl defmt::Format for RecvResult {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(
            f,
            "recv_len: {}, from_addr: {:?}, error: {}, return_offset: {}",
            self.recv_len,
            Ipv4AddrFormatWrapper::new(self.from_addr.ip()),
            self.error,
            self.return_offset
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(crate) struct BindListenResult {
    pub error: SocketError,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AcceptResult {
    pub accept_addr: core::net::SocketAddrV4,
    pub accepted_socket: Socket,
}
#[cfg(feature = "defmt")]
impl defmt::Format for AcceptResult {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(
            f,
            "accept_addr: {:?}, port: {}, accepted_socket: {:?}",
            Ipv4AddrFormatWrapper::new(self.accept_addr.ip()),
            self.accept_addr.port(),
            self.accepted_socket
        );
    }
}

#[derive(PartialEq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum AsyncState {
    Pending(Option<u32>),
    Done,
}

#[derive(PartialEq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum AsyncOp {
    Connect(Option<ConnectResult>),
    Send(SendRequest, Option<i16>),
    SendTo(SendRequest, Option<i16>),
    Recv(Option<RecvResult>),
    RecvFrom(Option<RecvResult>),
    Accept(Option<AcceptResult>),
}

#[derive(PartialEq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ClientSocketOp {
    None,
    New,
    Bind(Option<BindListenResult>),
    Listen(Option<BindListenResult>),
    AsyncOp(AsyncOp, AsyncState),
}

/// PRNG (Pseudorandom Number Generator) structure.
/// The `defmt` feature derives `defmt::Format` for logging/serialization if enabled.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Prng {
    pub offset: usize,
    pub rcv_buffer: Option<[u8; PRNG_DATA_LENGTH]>,
}

impl SocketCallbacks {
    pub fn new() -> Self {
        Self {
            tcp_sockets: SockHolder::new(),
            udp_sockets: SockHolder::new(),
            listening_sockets: [false; NUM_TCP_SOCKETS],
            accept_backlog: [None; NUM_TCP_SOCKETS],
            udp_socket_connect_addr: [None; MAX_UDP_SOCKETS],
            recv_buffer: [0; SOCKET_BUFFER_MAX_LENGTH],
            dns_resolved_addr: None,
            connection_state: ConnectionState::new(),
            state: WifiModuleState::Reset,
            prng: None,
            provisioning_info: None,
        }
    }
    pub fn resolve(&mut self, socket: Socket) -> Option<&mut (Socket, ClientSocketOp)> {
        if socket.v < NUM_TCP_SOCKETS as u8 {
            debug!("resolving tcp: {:?}", socket.v);
            self.tcp_sockets.get(Handle(socket.v))
        } else {
            debug!("resolving udp: {:?}", socket.v);
            self.udp_sockets
                .get(Handle(socket.v - NUM_TCP_SOCKETS as u8))
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
        self.dns_resolved_addr = Some(Some(ip));
    }

    fn on_default_connect(&mut self, status: WifiConnError) {
        debug!(
            "client: got connected {}",
            (status == WifiConnError::NoError)
        );

        if (self.state == WifiModuleState::ConnectingToAp) && (status != WifiConnError::NoError) {
            self.state = WifiModuleState::ConnectionFailed;
            self.connection_state.conn_error = Some(status);
        }
    }
    fn on_dhcp(&mut self, conf: crate::manager::IPConf) {
        debug!("client: on_dhcp: IP config: {}", conf);
        self.connection_state.ip_conf = Some(conf);
    }
    fn on_connstate_changed(&mut self, state: WifiConnState, err: WifiConnError) {
        debug!("client: Connection state changed: {:?} {:?}", state, err);
        self.connection_state.conn_state = state;
        self.connection_state.conn_error = Some(err);

        match self.connection_state.conn_state {
            WifiConnState::Connected => {
                if self.state != WifiModuleState::Provisioning {
                    self.state = WifiModuleState::ConnectedToAp;
                }
            }
            WifiConnState::Disconnected => {
                if self.state == WifiModuleState::ConnectingToAp {
                    self.state = WifiModuleState::ConnectionFailed;
                    debug!(
                        "on_connstate_changed FAILED: {:?} {:?}",
                        self.connection_state.conn_state, self.connection_state.conn_error
                    );
                } else if self.state != WifiModuleState::Provisioning {
                    self.state = WifiModuleState::Unconnected;
                }
            }
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
    fn on_connect(&mut self, socket: Socket, err: SocketError) {
        debug!("on_connect: socket {:?}", socket);
        match self.resolve(socket) {
            Some((
                _,
                ClientSocketOp::AsyncOp(
                    AsyncOp::Connect(option),
                    asyncstate @ AsyncState::Pending(_),
                ),
            )) => {
                option.replace(ConnectResult { error: err });
                *asyncstate = AsyncState::Done;
            }
            Some((s, op)) => error!(
                "UNKNOWN STATE on_connect (x): socket:{:?} error:{:?} state:{:?}",
                s, err, op
            ),
            None => error!(
                "on_connect (x): COULD NOT FIND SOCKET socket:{:?} error:{:?}",
                socket, err
            ),
        }
    }
    fn on_send_to(&mut self, socket: Socket, len: i16) {
        debug!("on_send_to: socket:{:?} length:{:?}", socket, len);
        match self.resolve(socket) {
            Some((
                s,
                ClientSocketOp::AsyncOp(
                    AsyncOp::SendTo(req, option),
                    asyncstate @ AsyncState::Pending(_),
                ),
            )) => {
                req.total_sent += len;
                req.remaining -= len;
                if req.remaining <= 0 {
                    debug!("FIN: on_send_to: socket:{:?} length:{:?}", s, len);
                    option.replace(len);
                    *asyncstate = AsyncState::Done;
                } else {
                    debug!("CONT: on_send_to: socket:{:?} length:{:?}", s, len);
                }
            }
            Some((s, op)) => error!(
                "UNKNOWN STATE on_send_to (x): socket:{:?} len:{:?} state:{:?}",
                s, len, *op
            ),
            None => error!(
                "on_send (x): COULD NOT FIND SOCKET socket:{:?} len:{:?}",
                socket, len
            ),
        }
    }
    fn on_send(&mut self, socket: Socket, len: i16) {
        debug!("on_send: socket {:?} len:{}", socket, len);
        match self.resolve(socket) {
            Some((
                s,
                ClientSocketOp::AsyncOp(
                    AsyncOp::Send(req, option),
                    asyncstate @ AsyncState::Pending(_),
                ),
            )) => {
                req.total_sent += len;
                req.remaining -= len;
                if req.remaining <= 0 {
                    debug!("FIN: on_send: socket:{:?} length:{:?}", s, len);
                    option.replace(len);
                    *asyncstate = AsyncState::Done;
                } else {
                    debug!("CONT: on_send: socket:{:?} length:{:?}", s, len);
                }
            }
            Some((s, op)) => error!(
                "UNKNOWN STATE on_send (x): socket:{:?} len:{:?} state:{:?}",
                s, len, *op
            ),
            None => error!(
                "on_send (x): COULD NOT FIND SOCKET socket:{:?} len:{:?}",
                socket, len
            ),
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
        match self.resolve(socket) {
            Some((s, ClientSocketOp::AsyncOp(
                    AsyncOp::Recv(option),
                    asyncstate @ AsyncState::Pending(_),))) => {
                debug!(
                    "on_recv: socket:{:?} address:{:?} data:{:?} len:{:?} error:{:?}",
                    s,
                    Ipv4AddrFormatWrapper::new(address.ip()),
                    data,
                    data.len(),
                    err
                );
                option.replace(RecvResult {
                    recv_len: data.len(),
                    from_addr: address,
                    error: err,
                    return_offset: 0,
                });
                *asyncstate = AsyncState::Done;
                self.recv_buffer[..data.len()].copy_from_slice(data);
            }
            Some((_, op)) => error!(
                "Socket NOT in recv: socket:{:?} address:{:?} data:{:?} error:{:?} actual state:{:?}",
                socket,
                Ipv4AddrFormatWrapper::new(address.ip()),
                data,
                err, op
            ),
            None => error!(
                "UNKNOWN on_recv: socket:{:?} address:{:?} data:{:?} error:{:?}",
                socket,
                Ipv4AddrFormatWrapper::new(address.ip()),
                data,
                err
            ),
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
        match self.resolve(socket) {
            Some((s, ClientSocketOp::AsyncOp(
                    AsyncOp::RecvFrom(option),
                    asyncstate @ AsyncState::Pending(_),))) => {
                debug!(
                    "on_recvfrom: raw:{:?} socket:{:?} address:{:?} data:{:?} error:{:?}",
                    socket,
                    s,
                    Ipv4AddrFormatWrapper::new(address.ip()),
                    data,
                    err
                );
                option.replace(RecvResult {
                    recv_len: data.len(),
                    from_addr: address,
                    error: err,
                    return_offset: 0,
                });
                *asyncstate = AsyncState::Done;
                self.recv_buffer[..data.len()].copy_from_slice(data);
            }
            Some((_, op)) => error!(
                "Socket NOT in recvfrom: socket:{:?} address:{:?} data:{:?} error:{:?} actual state:{:?}",
                socket,
                Ipv4AddrFormatWrapper::new(address.ip()),
                data,
                err,
                op
            ),
            None => error!(
                "UNKNOWN on_recvfrom: socket:{:?} address:{:?} data:{:?} error:{:?}",
                socket,
                Ipv4AddrFormatWrapper::new(address.ip()),
                data,
                err
            ),
        }
    }
    fn on_bind(&mut self, sock: Socket, err: SocketError) {
        debug!("on_bind: socket {:?}", sock);
        match self.resolve(sock) {
            Some((_, ClientSocketOp::Bind(option))) => {
                option.replace(BindListenResult { error: err });
            }
            Some((s, op)) => error!(
                "UNKNOWN on_bind: socket:{:?} error:{:?} state:{:?}",
                s, err, op
            ),
            None => error!("UNKNOWN socket on_bind: socket:{:?} error:{:?}", sock, err),
        }
    }
    fn on_listen(&mut self, sock: Socket, err: SocketError) {
        debug!("on_listen: socket {:?}", sock);
        match self.resolve(sock) {
            Some((_, ClientSocketOp::Listen(option))) => {
                option.replace(BindListenResult { error: err });
            }
            Some((s, op)) => error!(
                "UNKNOWN on_listen: socket:{:?} error:{:?} state:{:?}",
                s, err, op
            ),
            None => error!(
                "UNKNOWN socket on_listen: socket:{:?} error:{:?}",
                sock, err
            ),
        }
    }

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
        let was_listening = self.listening_sockets[listen_socket.v as usize];

        match self.resolve(listen_socket) {
            Some((s, ClientSocketOp::AsyncOp(
                    AsyncOp::Accept(option),
                    asyncstate @ AsyncState::Pending(_),))) => {
                debug!("on_accept: socket:{:?} port:{:?} address:{:?} accepted_socket:{:?}", s,
                    Ipv4AddrFormatWrapper::new(address.ip()),
                    address.port(),
                    accepted_socket);
                option.replace(AcceptResult {
                    accept_addr: address,
                    accepted_socket,
                });
                *asyncstate = AsyncState::Done;
            }
            // Todo here: If it was in None state, move it to accept backlog
            Some((_, op)) => {
                info!(
                    "Socket was NOT in accept: address:{:?} port:{:?} listen_socket:{:?} accepted_socket:{:?} actual state:{:?} listening:{:?}",
                    Ipv4AddrFormatWrapper::new(address.ip()),
                    address.port(),
                    listen_socket,
                    accepted_socket,
                    op,
                    was_listening
                );

                // If the socket was listening, we will mark it as backlogged new socket
                // and put it in tcp sock store
                if was_listening {
                    let accept_socket_id = accepted_socket.v;
                    let handle = self.tcp_sockets.put(
                        Handle(accept_socket_id ),accepted_socket.s
                    );
                    if handle.is_some() {
                        if self.accept_backlog[accept_socket_id as usize].is_none() {
                            self.accept_backlog[accept_socket_id as usize] = Some((accepted_socket, address));
                        } else {
                            error!("Failed to put socket {:?} in tcp sock store, collision in accept backlog ?", accept_socket_id);
                        }
                    } else {
                        error!("Failed to put socket in tcp sock store, collision in tcp sock store ?");
                    }
                } else {
                    error!("Socket was not listening on {:?}", listen_socket);
                }
            } ,
            None => error!(
                "UNKNOWN socket on_accept: address:{:?} port:{:?} listen_socket:{:?} accepted_socket:{:?}",
                Ipv4AddrFormatWrapper::new(address.ip()),
                address.port(),
                listen_socket,
                accepted_socket
            ),
        }
    }

    /// Callback function to store the random bytes read from the chip in the PRNG data.
    ///
    /// # Arguments
    ///
    /// * `data` - Random bytes read from the chip.
    fn on_prng(&mut self, data: &[u8]) {
        if let Some(Some(prng)) = self.prng.as_mut() {
            if let ref mut buffer @ None = prng.rcv_buffer {
                let mut new_buf = [0u8; PRNG_DATA_LENGTH];
                new_buf[..data.len()].copy_from_slice(data);
                *buffer = Some(new_buf);
            }
        }
    }

    /// Callback function to store the provisioning information.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID received from the chip.
    /// * `passphrase` - The Wi-Fi passphrase.
    /// * `security` - The security type (e.g., WPA2).
    /// * `status` - Provisioning status (`true` if successful).
    fn on_provisioning(&mut self, ssid: Ssid, key: WpaKey, security: AuthType, status: bool) {
        let mut info = ProvisioningInfo {
            ssid,
            key: Credentials::Open,
            status,
        };
        // Check if provisioning was successful
        if status {
            let mut cred = Credentials::Open;

            match security {
                AuthType::Open => cred = Credentials::Open,
                AuthType::WpaPSK => cred = Credentials::WpaPSK(key),
                #[cfg(feature = "wep")]
                AuthType::WEP => {
                    let mut wep_key = WepKey::new();
                    let key_len = key.len().min(wep_key.capacity());
                    wep_key.push_str(&key[..key_len]);
                    cred = Credentials::Wep(wep_key, WepKeyIndex::Key1);
                }
                _ => error!("Invalid or Unsupported Authentication type"),
            }
            // Update the Credentials
            info.key = cred;
        }
        self.provisioning_info = Some(Some(info));
    }
}
