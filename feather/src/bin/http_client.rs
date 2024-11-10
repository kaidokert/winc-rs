#![no_main]
#![no_std]

use bsp::shared::parse_ip_octets;
use stack::StackError;
use core::str::FromStr;
use feather as bsp;

use embedded_nal::nb::block;
use embedded_nal::{IpAddr, Ipv4Addr, SocketAddr};
use embedded_nal::TcpClientStack;

const DEFAULT_TEST_IP: &str = "192.168.1.1";
const DEFAULT_TEST_PORT: &str = "12345";
const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";

mod stack;
mod runner;

use runner::{connect_and_run, MyTcpClientStack};

fn http_client<T, S>(stack: &mut T, addr: Ipv4Addr, port: u16) -> Result<(), T::Error>
where
    T: TcpClientStack<TcpSocket = S> + ?Sized,
    T::Error: From<embedded_nal::nb::Error<T::Error>>,
{
    let sock = stack.socket();
    if let Ok(mut s) = sock {
        defmt::println!("-----connecting to ----- {}.{}.{}.{} port {}", addr.octets()[0], addr.octets()[1], addr.octets()[2], addr.octets()[3], port);
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

#[cortex_m_rt::entry]
fn main() -> ! {
    if let Err(something) = connect_and_run("Hello,http client", true, 
        |stack: MyTcpClientStack| -> Result<(), StackError> {
        let test_ip = option_env!("TEST_IP").unwrap_or(DEFAULT_TEST_IP);
        let ip_values: [u8; 4] = parse_ip_octets(test_ip);
        let ip = Ipv4Addr::new(ip_values[0], ip_values[1], ip_values[2], ip_values[3]);
        let test_port = option_env!("TEST_PORT").unwrap_or(DEFAULT_TEST_PORT);
        let port = u16::from_str(test_port).unwrap_or(12345);
        defmt::info!("---- Starting HTTP client ---- ");
        http_client(stack, ip, port)?;
        defmt::info!("---- HTTP Client done ---- ");
        Ok(())
    }, |_| Ok(()) ) {
        defmt::info!("Something went wrong {}", something)
    } else {
        defmt::info!("Good exit")
    };
    loop {}
}
