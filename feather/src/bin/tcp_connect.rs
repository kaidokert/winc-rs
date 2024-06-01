#![no_main]
#![no_std]

use bsp::shared::Stream;
use feather as bsp;
use bsp::hal::prelude::*;

use feather::init::init;

use cortex_m_systick_countdown::{PollingSysTick, SysTickCalibration};

use embedded_nal::{TcpClientStack, TcpError, TcpErrorKind};
use embedded_nal::{IpAddr,Ipv4Addr, SocketAddr};
use feather_m0::ehal::can::ErrorKind;
use embedded_nal::nb::block;


use wincwifi::transfer::Xfer;


const DEFAULT_TEST_IP: &str = "192.168.1.1";

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
    let test_ip = option_env!("TESTIP").unwrap_or(DEFAULT_TEST_IP);
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

#[cortex_m_rt::entry]
fn main() -> ! {



    if let Ok((mut delay, mut red_led, cs, spi)) = init() {
        defmt::println!("Hello, tcp_connect with shared init!");

        let delay_shim = | v: u32 | {

        };
        let stream = Stream::new(spi,delay_shim);
        //let xfer = RawXfer::new(stream, cs, spi);
    
        delay.delay_ms(2000u32);
        let _ = do_http();
        loop {
            delay.delay_ms(200u32);
            red_led.set_high().unwrap();
            delay.delay_ms(200u32);
            red_led.set_low().unwrap();
        }
    }
    loop {}
}