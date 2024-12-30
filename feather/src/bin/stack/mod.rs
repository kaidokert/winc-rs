use core::convert::Infallible;

use embedded_nal::{TcpClientStack, UdpClientStack};
use feather_m0::pac::dsu::addr;
use wincwifi::manager::EventListener;
use wincwifi::manager::SocketError;
use wincwifi::Ipv4AddrFormatWrapper;
use wincwifi::Socket;

use embedded_nal::nb;
use wincwifi::SockHolder;

use wincwifi::{ClientSocketOp, Handle};

struct SocketCallbacks {
    // #define TCP_SOCK_MAX										(7)
    // indexes 0-6
    tcp_sockets: SockHolder<7, 0>,
    // #define UDP_SOCK_MAX										4
    udp_sockets: SockHolder<4, 7>,
    recv_buffer: [u8; wincwifi::manager::SOCKET_BUFFER_MAX_LENGTH],
    recv_len: usize,
    // Todo: move this into socket
    last_error: SocketError,
    last_recv_addr: Option<wincwifi::SocketAddrV4>,
}

impl SocketCallbacks {
    pub fn new() -> Self {
        Self {
            tcp_sockets: SockHolder::new(),
            udp_sockets: SockHolder::new(),
            recv_buffer: [0; wincwifi::manager::SOCKET_BUFFER_MAX_LENGTH],
            recv_len: 0,
            last_error: SocketError::NoError,
            last_recv_addr: None
        }
    }
    fn resolve(&mut self, socket: Socket) -> Option<&mut (Socket, ClientSocketOp)> {
        if socket.v < 7 {
            defmt::debug!("resolving tcp: {:?}", socket.v);
            self.tcp_sockets.get(Handle(socket.v))
        } else {
            defmt::debug!("resolving udp: {:?}", socket.v);
            self.udp_sockets.get(Handle(socket.v - 7))
        }
    }
}

impl EventListener for SocketCallbacks {
    fn on_dhcp(&mut self, conf: wincwifi::manager::IPConf) {
        defmt::info!("on_dhcp: IP config: {}", conf);
    }
    fn on_connect(&mut self, socket: Socket, err: SocketError) {
        defmt::debug!("on_connect: socket {:?}", socket);

        if let Some((s, op)) = self.resolve(socket) {
            if *op == ClientSocketOp::Connect {
                defmt::debug!("on_connect: socket:{:?} error:{:?}", s, err);
                *op = ClientSocketOp::None;
                self.last_error = err;
            } else {
                defmt::error!(
                    "UNKNOWN STATE on_connect (x): socket:{:?} error:{:?} state:{:?}",
                    s,
                    err,
                    *op
                );
            }
        } else {
            defmt::error!(
                "on_connect (x): COULD NOT FIND SOCKET socket:{:?} error:{:?}",
                socket,
                err
            );
        }
    }
    fn on_send_to(&mut self, socket: Socket, len: i16) {
        defmt::debug!("on_send_to: socket:{:?} length:{:?}", socket, len);
        if let Some((s, op)) = self.resolve(socket) {
            if *op == ClientSocketOp::SendTo {
                defmt::debug!("on_send_to: socket:{:?} length:{:?}", socket, len);
                *op = ClientSocketOp::None;
            } else {
                defmt::error!(
                    "UNKNOWN STATE on_send_to (x): socket:{:?} len:{:?} state:{:?}",
                    s,
                    len,
                    *op
                );
            }
        } else {
            defmt::error!(
                "UNKNOWN STATE on_send_to (x): socket:{:?} len:{:?}",
                socket,
                len
            );
        }
    }
    fn on_send(&mut self, socket: Socket, len: i16) {
        defmt::debug!("on_send: socket {:?}", socket);

        if let Some((s, op)) = self.resolve(socket) {
            if *op == ClientSocketOp::Send {
                defmt::debug!("on_send: socket:{:?} length:{:?}", socket, len);
                *op = ClientSocketOp::None;
            } else {
                defmt::error!(
                    "UNKNOWN STATE on_send (x): socket:{:?} len:{:?} state:{:?}",
                    s,
                    len,
                    *op
                );
            }
        } else {
            defmt::error!(
                "on_send (x): COULD NOT FIND SOCKET socket:{:?} len:{:?}",
                socket,
                len
            );
        }
    }
    fn on_recv(
        &mut self,
        socket: Socket,
        address: wincwifi::SocketAddrV4,
        data: &[u8],
        err: SocketError,
    ) {
        defmt::debug!("on_recv: socket {:?}", socket);
        let mut found = false;
        if let Some((s, op)) = self.resolve(socket) {
            if *op == ClientSocketOp::Recv {
                defmt::debug!(
                    "on_recv: socket:{:?} address:{:?} data:{:?} error:{:?}",
                    s,
                    Ipv4AddrFormatWrapper::new(address.ip()),
                    data,
                    err
                );
                *op = ClientSocketOp::None;
                found = true;
            } else {
                defmt::error!(
                    "UNKNOWN on_recv: socket:{:?} address:{:?} port:{:?} data:{:?} error:{:?}",
                    socket,
                    Ipv4AddrFormatWrapper::new(address.ip()),
                    address.port(),
                    data,
                    err
                );
            }
        } else {
            defmt::error!(
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
        address: wincwifi::SocketAddrV4,
        data: &[u8],
        err: SocketError,
    ) {
        defmt::debug!("on_recvfrom: socket {:?}", socket);
        let mut found = false;
        if let Some((s, op)) = self.resolve(socket) {
            if *op == ClientSocketOp::RecvFrom {
                defmt::debug!(
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
                defmt::error!(
                    "UNKNOWN on_recvfrom: socket:{:?} address:{:?} data:{:?} error:{:?}",
                    socket,
                    Ipv4AddrFormatWrapper::new(address.ip()),
                    data,
                    err
                );
            }
        } else {
            defmt::error!(
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
        defmt::info!(
            "on_system_time: {}-{:02}-{:02} {:02}:{:02}:{:02}",
            year,
            month,
            day,
            hour,
            minute,
            second
        );
    }
}

#[derive(Debug, defmt::Format)]
pub enum StackError {
    MyWouldBlock,
    GeneralTimeout,
    ConnectTimeout,
    RecvTimeout,
    SendTimeout,
    AddingASocketFailed(i32),
    DispatchError(wincwifi::error::Error),
    ConnectSendFailed(wincwifi::error::Error),
    ReceiveFailed(wincwifi::error::Error),
    SendSendFailed(wincwifi::error::Error),
    SendCloseFailed(wincwifi::error::Error),
    WincWifiFail(wincwifi::error::Error),
    OpFailed(SocketError),
    Weirdness,
    // todo: implement TcpError::PipeClosed correctly
}

impl From<Infallible> for StackError {
    fn from(_: Infallible) -> Self {
        unreachable!()
    }
}

impl From<SocketError> for StackError {
    fn from(inner: SocketError) -> Self {
        Self::OpFailed(inner)
    }
}

impl From<wincwifi::error::Error> for StackError {
    fn from(inner: wincwifi::error::Error) -> Self {
        Self::WincWifiFail(inner)
    }
}

impl embedded_nal::TcpError for StackError {
    fn kind(&self) -> embedded_nal::TcpErrorKind {
        embedded_nal::TcpErrorKind::Other
    }
}

impl From<nb::Error<StackError>> for StackError {
    fn from(inner: nb::Error<StackError>) -> Self {
        match inner {
            nb::Error::WouldBlock => StackError::MyWouldBlock,
            nb::Error::Other(e) => e,
        }
    }
}

pub struct WincClient<'a, X: wincwifi::transfer::Xfer, E: EventListener> {
    manager: wincwifi::manager::Manager<X, E>,
    delay: &'a mut dyn FnMut(u32) -> (),
    recv_timeout: u32,
    poll_loop_delay: u32,
    callbacks: SocketCallbacks,
    next_session_id: u16,
    // TODO: Lets change that per socket
    last_send_addr: Option<wincwifi::SocketAddrV4>,
}

impl<'a, X: wincwifi::transfer::Xfer, E: EventListener> WincClient<'a, X, E> {
    const SEND_TIMEOUT: u32 = 1000;
    const RECV_TIMEOUT: u32 = 1000;
    const CONNECT_TIMEOUT: u32 = 1000;
    const POLL_LOOP_DELAY: u32 = 100;
    pub fn new(manager: wincwifi::manager::Manager<X, E>, delay: &'a mut impl FnMut(u32)) -> Self {
        Self {
            manager,
            callbacks: SocketCallbacks::new(),
            delay,
            recv_timeout: Self::RECV_TIMEOUT,
            poll_loop_delay: Self::POLL_LOOP_DELAY,
            next_session_id: 0,
            last_send_addr: None
        }
    }
    pub fn get_next_session_id(&mut self) -> u16 {
        let ret = self.next_session_id;
        self.next_session_id += 1;
        ret
    }
    pub fn dispatch_events(&mut self) -> Result<(), StackError> {
        self.manager
            .dispatch_events_new(&mut self.callbacks)
            .map_err(|some_err| StackError::DispatchError(some_err))
    }
    // What could possible go wrong in this wait ? Should this be nb::block! ? 
    fn wait_for_op_ack(
        &mut self,
        handle: Handle,
        expect_op: ClientSocketOp,
        timeout: u32,
        tcp: bool // todo: this is ugly
    ) -> Result<usize, StackError> {
        self.dispatch_events()?;
        let mut timeout = timeout as i32;
        defmt::debug!("===>Waiting for op ack for {:?}", expect_op);
        loop {
            if timeout <= 0 {
                return match expect_op {
                    ClientSocketOp::Connect => return Err(StackError::ConnectTimeout),
                    ClientSocketOp::Send => return Err(StackError::SendTimeout),
                    ClientSocketOp::Recv => return Err(StackError::RecvTimeout),
                    _ => Err(StackError::GeneralTimeout),
                };
            }
            let (_sock, op) = match tcp {
                true => self.callbacks.tcp_sockets.get(handle).unwrap(),
                false => self.callbacks.udp_sockets.get(handle).unwrap(),
            };
            if *op == ClientSocketOp::None {
                defmt::debug!(
                    "<===Ack received {:?}, recv_len:{:?}",
                    *op,
                    self.callbacks.recv_len
                );
                if self.callbacks.last_error != SocketError::NoError {
                    return Err(StackError::OpFailed(self.callbacks.last_error));
                }
                return Ok(self.callbacks.recv_len);
            }
            (self.delay)(self.poll_loop_delay);
            self.dispatch_events()?;
            timeout -= self.poll_loop_delay as i32;
        }
    }
}

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
            .map_err(|x| StackError::AddingASocketFailed(x))?;
        Ok(handle)
    }
    fn connect(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        remote: embedded_nal::SocketAddr,
    ) -> Result<(), nb::Error<<Self as TcpClientStack>::Error>> {
        self.dispatch_events()?;
        match remote {
            embedded_nal::SocketAddr::V4(addr) => {
                let (sock, op) = self.callbacks.tcp_sockets.get(*socket).unwrap();
                *op = ClientSocketOp::Connect;
                let op = *op;
                defmt::info!("<> Sending send_socket_connect to {:?}", sock);
                self.manager
                    .send_socket_connect(*sock, addr)
                    .map_err(|x| StackError::ConnectSendFailed(x))?;
                self.wait_for_op_ack(*socket, op, Self::CONNECT_TIMEOUT, true)?;
            }
            embedded_nal::SocketAddr::V6(_) => unimplemented!("IPv6 not supported"),
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
        defmt::info!("<> Sending socket send_send to {:?}", sock);
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
        defmt::info!("<> Sending socket send_recv to {:?}", sock);
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
        let (sock, op) = self.callbacks.tcp_sockets.get(socket).unwrap();
        self.manager
            .send_close(*sock)
            .map_err(|x| StackError::SendCloseFailed(x))?;
        self.callbacks
            .tcp_sockets
            .get(socket)
            .ok_or(StackError::Weirdness)?;
        self.callbacks.tcp_sockets.remove(socket);
        Ok(())
    }
}

impl<'a, X: wincwifi::transfer::Xfer, E: EventListener> UdpClientStack for WincClient<'a, X, E> {
    type UdpSocket = Handle;

    type Error = StackError;

    fn socket(&mut self) -> Result<Self::UdpSocket, Self::Error> {
        defmt::info!("<> Calling new UDP socket");
        self.dispatch_events()?;
        let s = self.get_next_session_id();
        let handle = self
            .callbacks
            .udp_sockets
            .add(s)
            .map_err(|x| StackError::AddingASocketFailed(x))?;
        defmt::info!("<> Got handle {:?} ", handle.0);
        Ok(handle)
    }

    fn connect(
        &mut self,
        socket: &mut Self::UdpSocket,
        remote: embedded_nal::SocketAddr,
    ) -> Result<(), Self::Error> {
        self.dispatch_events()?;
        match remote {
            embedded_nal::SocketAddr::V4(addr) => {
                defmt::info!("<> Connect handle is {:?}", socket.0);
                let (sock, op) = self.callbacks.udp_sockets.get(*socket).unwrap();
                self.last_send_addr = Some(addr);
                /*
                *op = ClientSocketOp::Connect;
                let op = *op;
                defmt::info!("<> Sending udp send_socket_connect to {:?}", sock);
                self.manager
                    .send_socket_connect(*sock, addr)
                    .map_err(|x| StackError::ConnectSendFailed(x))?;
                defmt::info!("<> calling udp send_socket_connect opack {:?}", sock);
                self.wait_for_op_ack(*socket, op, Self::CONNECT_TIMEOUT, false)?;
                 */
            }
            embedded_nal::SocketAddr::V6(_) => unimplemented!("IPv6 not supported"),
        }
        Ok(())
    }

    fn send(&mut self, socket: &mut Self::UdpSocket, buffer: &[u8]) -> nb::Result<(), Self::Error> {
        self.dispatch_events()?;
        defmt::info!("<> in udp send {:?}", socket.0);
        let (sock, op) = self.callbacks.udp_sockets.get(*socket).unwrap();
        *op = ClientSocketOp::SendTo;
        let op = *op;
        defmt::info!("<> Sending socket udp send_send to {:?}", sock);
        if let Some(addr) = self.last_send_addr {
            self.manager
                .send_sendto(*sock, addr,buffer)
                .map_err(|x| StackError::SendSendFailed(x))?;
        } else {
            return Err(StackError::Weirdness.into());
        }
        self.wait_for_op_ack(*socket, op, Self::SEND_TIMEOUT, false)?;
        Ok(())
    }

    fn receive(
        &mut self,
        socket: &mut Self::UdpSocket,
        buffer: &mut [u8],
    ) -> nb::Result<(usize, embedded_nal::SocketAddr), Self::Error> {
        self.dispatch_events()?;
        let (sock, op) = self.callbacks.udp_sockets.get(*socket).unwrap();
        *op = ClientSocketOp::RecvFrom;
        let op = *op;
        let timeout = Self::RECV_TIMEOUT;
        defmt::info!("<> Sending udp socket send_recv to {:?}", sock);
        self.manager
            .send_recvfrom(*sock, timeout)
            .map_err(|x| StackError::ReceiveFailed(x))?;
        let recv_len = self.wait_for_op_ack(*socket, op, self.recv_timeout, false)?;
        {
            let dest_slice = &mut buffer[..recv_len];
            dest_slice.copy_from_slice(&self.callbacks.recv_buffer[..recv_len]);
        }
        // todo: propagate back the actual address
        let f = self.last_send_addr.unwrap();
        Ok((
            recv_len,
            embedded_nal::SocketAddr::V4(f),
        ))
    }

    fn close(&mut self, socket: Self::UdpSocket) -> Result<(), Self::Error> {
        self.dispatch_events()?;
        let (sock, op) = self.callbacks.udp_sockets.get(socket).unwrap();
        self.manager
            .send_close(*sock)
            .map_err(|x| StackError::SendCloseFailed(x))?;
        self.callbacks
            .udp_sockets
            .get(socket)
            .ok_or(StackError::Weirdness)?;
        self.callbacks.udp_sockets.remove(socket);
        Ok(())
    }
}
