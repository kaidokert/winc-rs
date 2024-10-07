#![no_main]
#![no_std]

use bsp::hal::prelude::*;
use bsp::shared::{create_delay_closure, SpiStream};
use wincwifi::Socket;
use core::convert::Infallible;
use feather as bsp;

use feather::init::init;

use cortex_m_systick_countdown::{MillisCountDown, PollingSysTick, SysTickCalibration};

use embedded_nal::nb::block;
use embedded_nal::{IpAddr, Ipv4Addr, SocketAddr};
use embedded_nal::{TcpClientStack, TcpError, TcpErrorKind};

use wincwifi::manager::{AuthType, EventListener, Manager, SocketError};


const DEFAULT_TEST_IP: &str = "192.168.1.1";
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

#[derive(Debug)]
struct myErr {}

struct mySocket {
    id: u8
}

impl TcpError for myErr {
    fn kind(&self) -> TcpErrorKind {
        todo!()
    }
}

struct stub<X: wincwifi::transfer::Xfer, E: EventListener> {
    tcp_sockets_ids: [bool;4],
    manager: Manager<X, E>
}

impl<X: wincwifi::transfer::Xfer, E: EventListener> stub<X,E> { 
    fn dispatch_events(&mut self) -> Result<(), myErr> {
        //self.manager.dispatch_events_new(&mut Some(self)).map_err(|some_err|  myErr {})
        Ok(())
    }
}

impl<X: wincwifi::transfer::Xfer, E: EventListener> EventListener for &mut stub<X,E> {}

impl<X: wincwifi::transfer::Xfer, E: EventListener> EventListener for stub<X,E> {
    fn on_connect(&mut self, socket: Socket, err: SocketError) {
        defmt::debug!("on_connect: socket:{:?} error:{:?}", socket, err)
    }
}

impl<X: wincwifi::transfer::Xfer, E: EventListener> embedded_nal::TcpClientStack for stub<X,E> {
    type TcpSocket = mySocket;
    type Error = myErr;
    fn socket(
        &mut self,
    ) -> Result<<Self as TcpClientStack>::TcpSocket, <Self as TcpClientStack>::Error> {
        // Grab a new socket number
        self.dispatch_events()?;
        Ok(mySocket { id: 0 })
    }
    fn connect(
        &mut self,
        _: &mut <Self as TcpClientStack>::TcpSocket,
        _: embedded_nal::SocketAddr,
    ) -> Result<(), embedded_nal::nb::Error<<Self as TcpClientStack>::Error>> {
        self.dispatch_events()?;
        // this needs to call send_socket_connect
        // e.g we need to have a reference to the manager
        Ok(())
    }
    fn send(
        &mut self,
        _: &mut <Self as TcpClientStack>::TcpSocket,
        _: &[u8],
    ) -> Result<usize, embedded_nal::nb::Error<<Self as TcpClientStack>::Error>> {
        self.dispatch_events()?;
        Ok(4)
    }
    fn receive(
        &mut self,
        _: &mut <Self as TcpClientStack>::TcpSocket,
        _: &mut [u8],
    ) -> Result<usize, embedded_nal::nb::Error<<Self as TcpClientStack>::Error>> {
        self.dispatch_events()?;
        Ok(4)
    }
    fn close(
        &mut self,
        _: <Self as TcpClientStack>::TcpSocket,
    ) -> Result<(), <Self as TcpClientStack>::Error> {
        self.dispatch_events()?;
        Ok(())
    }
}

fn generic_http_client<T, S>(stack: &mut T) -> Result<(), T::Error>
    where T: TcpClientStack<TcpSocket = S>
{
    let sock = stack.socket();
    if let Ok(mut s) = sock {
        let remote = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 80);
        let _ = stack.connect(&mut s, remote);
        defmt::println!("Socket connected");
        let http_get: &str = "GET / HTTP/1.1\r\n\r\n";
        let nbytes = stack.send(&mut s, http_get.as_bytes());
        defmt::println!("Request sent {}", nbytes.unwrap());
        let mut respbuf = [0; 1500];
        let resplen = block!(stack.receive(&mut s, &mut respbuf))?;
    } else {
        defmt::println!("Socket creation failed");
    }
    Ok(())
}

pub struct Callbacks;
impl EventListener for Callbacks {
    fn on_dhcp(&mut self, conf: wincwifi::manager::IPConf) {
        defmt::info!("on_dhcp: IP config: {}", conf);
    }
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
    fn format(&self, f: defmt::Formatter) {
        todo!()
        }
}

fn program() -> Result<(), MainError> {
    if let Ok((delay_tick, mut red_led, cs, spi)) = init() {
        defmt::println!("Hello, tcp_connect with shared init!");

        let mut countdown1 = MillisCountDown::new(&delay_tick);
        let mut countdown2 = MillisCountDown::new(&delay_tick);
        let mut delay_ms = create_delay_closure(&mut countdown1);

        let mut manager = Manager::from_xfer(
            SpiStream::new(cs, spi, create_delay_closure(&mut countdown2)),
            Callbacks {},
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

        delay_ms(2000u32);

        let test_ip = option_env!("TEST_IP").unwrap_or(DEFAULT_TEST_IP);
        let ip_values: [u8; 4] = parse_ip_octets(test_ip);
        let mut stack = stub {
            tcp_sockets_ids: [false; 4],
            manager
        };
        generic_http_client(&mut stack).map_err(|err| MainError::Any)?;

        loop {
            stack.dispatch_events().map_err(|err| MainError::Any)?;

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
