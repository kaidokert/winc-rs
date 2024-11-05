#![no_main]
#![no_std]

use bsp::hal::prelude::*;
use bsp::shared::{create_delay_closure, SpiStream};
use core::convert::Infallible;
use core::str::FromStr;
use feather as bsp;
use wincwifi::Socket;

use feather::init::init;

use cortex_m_systick_countdown::MillisCountDown;

use embedded_nal::nb::block;
use embedded_nal::{IpAddr, Ipv4Addr, SocketAddr};
use embedded_nal::{TcpClientStack, TcpError, TcpErrorKind};

use wincwifi::manager::{AuthType, EventListener, Manager, SocketError};
use wincwifi::Ipv4AddrFormatWrapper;

const DEFAULT_TEST_IP: &str = "192.168.1.1";
const DEFAULT_TEST_PORT: &str = "12345";
const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";

fn parse_ip_octets(ip: &str) -> [u8; 4] {
    let mut octets = [0; 4];
    let mut octet_index = 0;
    let mut current_value = 0;

    ip.bytes().for_each(|byte| match byte {
        b'0'..=b'9' => current_value = current_value * 10 + (byte - b'0'),
        b'.' => {
            octets[octet_index] = current_value;
            octet_index += 1;
            current_value = 0;
        }
        _ => {}
    });

    octets[octet_index] = current_value;
    octets
}

pub mod socketthings;
use socketthings::{ClientSocketOp, Handle};

// Todo: Merge this back into wincwifi::Socket
struct MySocket {
    sock: wincwifi::Socket,
    op: ClientSocketOp,
    recv_len: usize,
}

impl MySocket {
    fn new(handle: u8, session: u16) -> Self {
        Self {
            sock: wincwifi::Socket {
                v: handle,
                s: session,
            },
            op: ClientSocketOp::New,
            recv_len: 0,
        }
    }
}

struct SockHolder<const N: usize, const BASE: usize> {
    sock: [Option<MySocket>; N],
}

impl<const N: usize, const BASE: usize> SockHolder<N, BASE> {
    pub fn new() -> Self {
        Self {
            sock: core::array::from_fn(|_| None),
        }
    }
    fn len(&self) -> usize {
        self.sock.iter().filter(|a| a.is_some()).count()
    }
    fn add(&mut self, session_id: u16) -> Result<Handle, i32> {
        if self.len() >= N {
            return Err(-1);
        }
        for (index, element) in self.sock.iter_mut().enumerate() {
            if element.is_none() {
                let ns = MySocket::new((BASE + index) as u8, session_id);
                element.replace(ns);
                return Ok(Handle(index as u8));
            }
        }
        Err(-1)
    }
    fn remove(&mut self, handle: Handle) {
        self.sock[handle.0 as usize] = None;
    }
    fn get(&mut self, handle: Handle) -> Option<&mut MySocket> {
        self.sock[handle.0 as usize].as_mut()
    }
}

#[derive(Debug, defmt::Format)]
enum MyErr {
    MyWouldBlock,
    TcpError,
    AddingASocketFailed(i32),
    DispatchError(wincwifi::error::Error),
    ConnectSendFailed(wincwifi::error::Error),
    ReceiveFailed(wincwifi::error::Error),
    SendSendFailed(wincwifi::error::Error),
    SendCloseFailed(wincwifi::error::Error),
    Weirdness,
}

impl TcpError for MyErr {
    fn kind(&self) -> TcpErrorKind {
        TcpErrorKind::Other
    }
}

impl From<embedded_nal::nb::Error<MyErr>> for MyErr {
    fn from(inner: embedded_nal::nb::Error<MyErr>) -> Self {
        match inner {
            embedded_nal::nb::Error::WouldBlock => MyErr::MyWouldBlock,
            embedded_nal::nb::Error::Other(e) => e,
        }
    }
}

pub struct Callbacks {
    next_session_id: u16,
    tcp_sockets: SockHolder<7, 0>,
    udp_sockets: SockHolder<3, 7>,
    recv_buffer: [u8; wincwifi::manager::SOCKET_BUFFER_MAX_LENGTH],
}

impl Callbacks {
    pub fn new() -> Self {
        Self {
            next_session_id: 0,
            tcp_sockets: SockHolder::new(),
            udp_sockets: SockHolder::new(),
            recv_buffer: [0; wincwifi::manager::SOCKET_BUFFER_MAX_LENGTH],
        }
    }
    pub fn get_next_session_id(&mut self) -> u16 {
        let ret = self.next_session_id;
        self.next_session_id += 1;
        ret
    }
}

struct Stack<'a, X: wincwifi::transfer::Xfer, E: EventListener> {
    manager: Manager<X, E>,
    delay: &'a mut dyn FnMut(u32) -> (),
    recv_timeout: u32,
    callbacks: Callbacks,
}


impl EventListener for Callbacks {
    fn on_dhcp(&mut self, conf: wincwifi::manager::IPConf) {
        defmt::info!("on_dhcp: IP config: {}", conf);
    }
    fn on_connect(&mut self, socket: Socket, err: SocketError) {
        if let Some(s) = self.tcp_sockets.get(Handle(socket.v)) {
            if s.op == ClientSocketOp::Connect {
                defmt::debug!("on_connect: socket:{:?} error:{:?}", s.sock, err);
                s.op = ClientSocketOp::None;
            } else {
                defmt::error!(
                    "UNKNOWN STATE on_connect (x): socket:{:?} error:{:?} state:{:?}",
                    s.sock,
                    err,
                    s.op
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
        defmt::debug!("on_send_to: socket:{:?} length:{:?}", socket, len)
    }
    fn on_send(&mut self, socket: Socket, len: i16) {
        if let Some(s) = self.tcp_sockets.get(Handle(socket.v)) {
            if s.op == ClientSocketOp::Send {
                defmt::debug!("on_send: socket:{:?} length:{:?}", socket, len);
                s.op = ClientSocketOp::None;
            } else {
                defmt::error!(
                    "UNKNOWN STATE on_send (x): socket:{:?} len:{:?} state:{:?}",
                    s.sock,
                    len,
                    s.op
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
        let sock = self.tcp_sockets.get(Handle(socket.v));
        if let Some(s) = sock {
            if s.op == ClientSocketOp::Recv {
                defmt::debug!(
                    "on_recv: socket:{:?} address:{:?} data:{:?} error:{:?}",
                    s.sock,
                    Ipv4AddrFormatWrapper::new(address.ip()),
                    data,
                    err
                );
                self.recv_buffer[..data.len()].copy_from_slice(data);
                // s.recv_buffer.copy_from_slice(data);
                s.recv_len = data.len();
                s.op = ClientSocketOp::None;
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
    }
    fn on_recvfrom(
        &mut self,
        socket: Socket,
        address: wincwifi::SocketAddrV4,
        data: &[u8],
        err: SocketError,
    ) {
        let sock = self.tcp_sockets.get(Handle(socket.v));
        if let Some(s) = sock {
            if s.op == ClientSocketOp::Recv {
                defmt::debug!(
                    "on_recvfrom: socket:{:?} address:{:?} data:{:?} error:{:?}",
                    s.sock,
                    Ipv4AddrFormatWrapper::new(address.ip()),
                    data,
                    err
                );
                return;
            }
        }
        defmt::error!(
            "UNKNOWN on_recvfrom: socket:{:?} address:{:?} data:{:?} error:{:?}",
            socket,
            Ipv4AddrFormatWrapper::new(address.ip()),
            data,
            err
        );
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

impl<'a, X: wincwifi::transfer::Xfer, E: EventListener> Stack<'a, X, E> {
    const SEND_TIMEOUT: u32 = 1000;
    const RECV_TIMEOUT: u32 = 1000;
    const CONNECT_TIMEOUT: u32 = 1000;
    fn new(manager: Manager<X, E>, delay: &'a mut impl FnMut(u32)) -> Self {
        Self {
            manager,
            callbacks: Callbacks::new(),
            delay,
            recv_timeout: Self::RECV_TIMEOUT,
        }
    }
    fn dispatch_events(&mut self) -> Result<(), MyErr> {
        self.manager
            .dispatch_events_new(&mut self.callbacks)
            .map_err(|some_err| MyErr::DispatchError(some_err))
    }
    fn wait_for_op_ack(
        &mut self,
        handle: Handle,
        op: ClientSocketOp,
        timeout: u32,
    ) -> Result<usize, MyErr> {
        self.dispatch_events()?;
        let mut timeout = timeout as i32;
        const LOOP_DELAY: u32 = 100;
        defmt::debug!("===>Waiting for op ack for {:?}", op);
        loop {
            if timeout <= 0 {
                return Err(MyErr::TcpError);
            }
            let sock = self.callbacks.tcp_sockets.get(handle).unwrap();
            if sock.op == ClientSocketOp::None {
                defmt::debug!(
                    "<===Ack received {:?}, sock.recv_len:{:?}",
                    op,
                    sock.recv_len
                );
                return Ok(sock.recv_len);
            }
            (self.delay)(LOOP_DELAY);
            self.dispatch_events()?;
            timeout -= LOOP_DELAY as i32;
        }
    }
}

impl<'a, X: wincwifi::transfer::Xfer, E: EventListener> embedded_nal::TcpClientStack
    for Stack<'a, X, E>
{
    type TcpSocket = Handle;
    type Error = MyErr;
    fn socket(
        &mut self,
    ) -> Result<<Self as TcpClientStack>::TcpSocket, <Self as TcpClientStack>::Error> {
        self.dispatch_events()?;
        let s = self.callbacks.get_next_session_id();
        let handle = self
            .callbacks
            .tcp_sockets
            .add(s)
            .map_err(|x| MyErr::AddingASocketFailed(x))?;
        Ok(handle)
    }
    fn connect(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        remote: embedded_nal::SocketAddr,
    ) -> Result<(), embedded_nal::nb::Error<<Self as TcpClientStack>::Error>> {
        self.dispatch_events()?;
        match remote {
            embedded_nal::SocketAddr::V4(addr) => {
                let sock = self.callbacks.tcp_sockets.get(*socket).unwrap();
                sock.op = ClientSocketOp::Connect;
                let op = sock.op;
                defmt::info!("<> Sending send_socket_connect to {:?}", sock.sock);
                self.manager
                    .send_socket_connect(sock.sock, addr)
                    .map_err(|x| MyErr::ConnectSendFailed(x))?;
                self.wait_for_op_ack(*socket, op, Self::CONNECT_TIMEOUT)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn send(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        data: &[u8],
    ) -> Result<usize, embedded_nal::nb::Error<<Self as TcpClientStack>::Error>> {
        self.dispatch_events()?;
        let sock = self.callbacks.tcp_sockets.get(*socket).unwrap();
        sock.op = ClientSocketOp::Send;
        let op = sock.op;
        defmt::info!("<> Sending socket send_send to {:?}", sock.sock);
        self.manager
            .send_send(sock.sock, data)
            .map_err(|x| MyErr::SendSendFailed(x))?;
        self.wait_for_op_ack(*socket, op, Self::SEND_TIMEOUT)?;
        Ok(data.len())
    }
    fn receive(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        data: &mut [u8],
    ) -> Result<usize, embedded_nal::nb::Error<<Self as TcpClientStack>::Error>> {
        self.dispatch_events()?;
        let sock = self.callbacks.tcp_sockets.get(*socket).unwrap();
        sock.op = ClientSocketOp::Recv;
        let op = sock.op;
        let timeout = 1000_i32;
        defmt::info!("<> Sending socket send_recv to {:?}", sock.sock);
        self.manager
            .send_recv(sock.sock, timeout as u32)
            .map_err(|x| MyErr::ReceiveFailed(x))?;
        let recv_len = self.wait_for_op_ack(*socket, op, self.recv_timeout)?;
        {
            let dest_slice = &mut data[..recv_len];
            dest_slice.copy_from_slice(&self.callbacks.recv_buffer[..recv_len]);
        }
        Ok(recv_len)
    }
    fn close(
        &mut self,
        socket: <Self as TcpClientStack>::TcpSocket,
    ) -> Result<(), <Self as TcpClientStack>::Error> {
        self.dispatch_events()?;
        let sock = self.callbacks.tcp_sockets.get(socket).unwrap();
        self.manager
            .send_close(sock.sock)
            .map_err(|x| MyErr::SendCloseFailed(x))?;
        self.callbacks
            .tcp_sockets
            .get(socket)
            .ok_or(MyErr::Weirdness)?;
        self.callbacks.tcp_sockets.remove(socket);
        Ok(())
    }
}

fn generic_http_client<T, S>(stack: &mut T, addr: Ipv4Addr, port: u16) -> Result<(), T::Error>
where
    T: TcpClientStack<TcpSocket = S>,
    T::Error: From<embedded_nal::nb::Error<T::Error>>,
{
    let sock = stack.socket();
    if let Ok(mut s) = sock {
        let remote = SocketAddr::new(IpAddr::V4(addr), port);
        stack.connect(&mut s, remote)?;
        defmt::println!("-----Socket connected-----");
        let http_get: &str = "GET /v1 HTTP/1.1\r\n\r\n";
        let nbytes = stack.send(&mut s, http_get.as_bytes());
        defmt::println!("-----Request sent {}-----", nbytes.unwrap());
        let mut respbuf = [0; 1500];
        let resplen = block!(stack.receive(&mut s, &mut respbuf))?;
        defmt::println!("-----Response received {}-----", resplen);
        let the_received_slice = &respbuf[..resplen];
        let recvd_str = core::str::from_utf8(the_received_slice).unwrap();
        defmt::println!("-----Response: {}-----", recvd_str);
        stack.close(s)?;
    } else {
        defmt::println!("Socket creation failed");
    }
    Ok(())
}

pub enum MainError {
    Any,
    Winc(wincwifi::error::Error),
}
impl From<Infallible> for MainError {
    fn from(_: Infallible) -> Self {
        unreachable!("Infallible error")
    }
}
impl From<wincwifi::error::Error> for MainError {
    fn from(e: wincwifi::error::Error) -> Self {
        Self::Winc(e)
    }
}
#[cfg(not(feature = "std"))]
impl defmt::Format for MainError {
    fn format(&self, _f: defmt::Formatter) {
        todo!()
    }
}

fn program() -> Result<(), MainError> {
    if let Ok((delay_tick, mut red_led, cs, spi)) = init() {
        defmt::println!("Hello, tcp_connect with shared init!");

        let mut countdown1 = MillisCountDown::new(&delay_tick);
        let mut countdown2 = MillisCountDown::new(&delay_tick);
        let mut countdown3 = MillisCountDown::new(&delay_tick);
        let mut delay_ms = create_delay_closure(&mut countdown1);
        let mut delay_ms2 = create_delay_closure(&mut countdown3);

        let mut manager = Manager::from_xfer(
            SpiStream::new(cs, spi, create_delay_closure(&mut countdown2)),
            Callbacks::new(),
        );
        manager.set_crc_state(true);

        manager.start(&mut |v: u32| -> bool {
            defmt::debug!("Waiting start .. {}", v);
            delay_ms(40);
            false
        })?;
        defmt::debug!("Chip started..");

        let ssid = option_env!("TEST_SSID").unwrap_or(DEFAULT_TEST_SSID);
        let password = option_env!("TEST_PASSWORD").unwrap_or(DEFAULT_TEST_PASSWORD);

        manager.send_connect(AuthType::WpaPSK, ssid, password, 0xFF, false)?;

        for _ in 0..10 {
            manager.dispatch_events()?;
            delay_ms(300u32);
        }

        let test_ip = option_env!("TEST_IP").unwrap_or(DEFAULT_TEST_IP);
        let ip_values: [u8; 4] = parse_ip_octets(test_ip);
        let ip = Ipv4Addr::new(ip_values[0], ip_values[1], ip_values[2], ip_values[3]);
        let mut stack = Stack::new(manager, &mut delay_ms2);
        let test_port = option_env!("TEST_PORT").unwrap_or(DEFAULT_TEST_PORT);
        let port = u16::from_str(test_port).unwrap_or(12345);
        defmt::info!("---- Starting HTTP client ---- ");
        generic_http_client(&mut stack, ip, port).map_err(|_err| MainError::Any)?;
        defmt::info!("---- HTTP Client done ---- ");
        loop {
            stack.dispatch_events().map_err(|_err| MainError::Any)?;

            delay_ms(200u32);
            red_led.set_high()?;
            delay_ms(200u32);
            red_led.set_low()?;
        }
    }
    Ok(())
}

#[cortex_m_rt::entry]
fn main() -> ! {
    if let Err(something) = program() {
        defmt::info!("Bad error {}", something)
    } else {
        defmt::info!("Good exit")
    };
    loop {}
}
