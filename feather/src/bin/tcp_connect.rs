#![no_main]
#![no_std]

use bsp::shared::SpiStream;
use feather as bsp;
use bsp::hal::prelude::*;
use core::convert::Infallible;

use feather::init::init;

use cortex_m_systick_countdown::{MillisCountDown, PollingSysTick, SysTickCalibration};

use embedded_nal::{TcpClientStack, TcpError, TcpErrorKind};
use embedded_nal::{IpAddr,Ipv4Addr, SocketAddr};
use embedded_nal::nb::block;

use wincwifi::manager::{Manager, EventListener,AuthType};

//use wincwifi::transfer::{ReadWrite, Xfer};

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

struct stub {

}

#[derive(Debug)]
struct myErr {}

struct mySocket {}

impl TcpError for myErr {
    fn kind(&self) -> TcpErrorKind { todo!() }
}

impl embedded_nal::TcpClientStack for stub {
    type TcpSocket = mySocket;
    type Error = myErr;
    fn socket(&mut self) -> Result<<Self as TcpClientStack>::TcpSocket, <Self as TcpClientStack>::Error> { 
        Ok(mySocket{})
    }
    fn connect(&mut self, _: &mut <Self as TcpClientStack>::TcpSocket, _: embedded_nal::SocketAddr) -> Result<(), embedded_nal::nb::Error<<Self as TcpClientStack>::Error>> { 
        Ok(())
    }
    fn send(&mut self, _: &mut <Self as TcpClientStack>::TcpSocket, _: &[u8]) -> Result<usize, embedded_nal::nb::Error<<Self as TcpClientStack>::Error>> {
         Ok(4)
    }
    fn receive(&mut self, _: &mut <Self as TcpClientStack>::TcpSocket, _: &mut [u8]) -> Result<usize, embedded_nal::nb::Error<<Self as TcpClientStack>::Error>> { 
        Ok(4)
    }
    fn close(&mut self, _: <Self as TcpClientStack>::TcpSocket) -> Result<(), <Self as TcpClientStack>::Error> { 
        Ok(())
    }
}

fn do_http() -> Result<u8,myErr> {
    let test_ip = option_env!("TEST_IP").unwrap_or(DEFAULT_TEST_IP);
    let ip_values: [u8; 4] = parse_ip_octets(test_ip);
    let mut stack = stub {};
    let sock = stack.socket();
    if let Ok(mut s) = sock {
        defmt::println!("Socket created");
        let remote =  SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 80);
        let _ = stack.connect(&mut s, remote);
        defmt::println!("Socket connected");
        let http_get: &str = "GET / HTTP/1.1\r\n\r\n";
        let nbytes = stack.send(&mut s, http_get.as_bytes());
        defmt::println!("Request sent {}",nbytes.unwrap());
        let mut respbuf = [0; 1500];
        let resplen = block!(stack.receive(&mut s, &mut respbuf))?;
    } else {
        defmt::println!("Socket creation failed");
    }
    Ok(1)
}

pub struct Callbacks;
impl EventListener for Callbacks {
    fn on_dhcp(&mut self, conf: wincwifi::manager::IPConf) {
        defmt::info!("on_dhcp: IP config: {}", conf);
    }
}

#[derive(defmt::Format)]
pub enum MainError {
    Winc(wincwifi::error::Error),
}
impl From<Infallible> for MainError {
    fn from(_: Infallible) -> Self {
        todo!()
    }
}
impl From<wincwifi::error::Error> for MainError {
    fn from(e: wincwifi::error::Error) -> Self {
        Self::Winc(e)
    }
}



fn program() -> Result<(),  MainError> {
    if let Ok((mut delay, mut red_led, cs, spi)) = init() {
        let mut countdown1 = MillisCountDown::new(&delay);
        let mut countdown2 = MillisCountDown::new(&delay);

        defmt::println!("Hello, tcp_connect with shared init!");

        let delay_shim = | v: u32 | {
            countdown1.start_ms(v);
            nb::block!(countdown1.wait()).unwrap();
        };
        let mut delay2 = | v: u32 | {
            countdown2.start_ms(v);
            nb::block!(countdown2.wait()).unwrap();
        };
        let mut manager = Manager::from_xfer(SpiStream::new(cs, spi,delay_shim), Callbacks {});
        manager.set_crc_state(true);

        manager.start(&mut |v: u32| -> bool {
            defmt::debug!("Waiting start .. {}", v);
            delay2(40);
            false
        })?;
        defmt::debug!("Chip started..");

        let ssid = option_env!("TEST_SSID").unwrap_or(DEFAULT_TEST_SSID);
        let password = option_env!("TEST_PASSWORD").unwrap_or(DEFAULT_TEST_PASSWORD);

        manager.send_connect(AuthType::WpaPSK, ssid, password, 0xFF, false)?;

        delay2(2000u32);
        let _ = do_http();
        loop {
            
            manager.dispatch_events()?;

            delay2(200u32);
            red_led.set_high().unwrap();
            delay2(200u32);
            red_led.set_low().unwrap();
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