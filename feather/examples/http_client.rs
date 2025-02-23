//! Make a very basic HTTP get request
//!
//! IP, port, and hostname are passed as env vars at build time

#![no_main]
#![no_std]

use bsp::shared::parse_ip_octets;
use core::str::FromStr;
use feather as bsp;
use wincwifi::StackError;

use core::net::Ipv4Addr;

use demos::http_client;

mod runner;
use runner::{connect_and_run, ClientType, ReturnClient};

const DEFAULT_TEST_IP: &str = "192.168.1.1";
const DEFAULT_TEST_PORT: &str = "12345";
const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";

#[cortex_m_rt::entry]
fn main() -> ! {
    if let Err(something) = connect_and_run(
        "Hello HTTP client",
        ClientType::Tcp,
        |stack: ReturnClient, _: core::net::Ipv4Addr| -> Result<(), StackError> {
            if let ReturnClient::Tcp(stack) = stack {
                let test_ip = option_env!("TEST_IP").unwrap_or(DEFAULT_TEST_IP);
                let ip_values: [u8; 4] = parse_ip_octets(test_ip);
                let ip = Ipv4Addr::new(ip_values[0], ip_values[1], ip_values[2], ip_values[3]);
                let test_port = option_env!("TEST_PORT").unwrap_or(DEFAULT_TEST_PORT);
                let port = u16::from_str(test_port).unwrap_or(12345);

                let test_host = option_env!("TEST_HOST");

                defmt::info!("---- Starting HTTP client ---- ");
                http_client::http_client(stack, ip, port, test_host)?;
                defmt::info!("---- HTTP Client done ---- ");
            }
            Ok(())
        },
    ) {
        defmt::error!("Something went wrong {}", something)
    } else {
        defmt::info!("Good exit")
    };
    loop {}
}
