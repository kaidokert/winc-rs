#![no_main]
#![no_std]
#![allow(unused_imports)]

use core::str::FromStr;
use feather as bsp;

use wincwifi::StackError;

use demos::http_server;

mod runner;
use runner::{connect_and_run, ClientType, ReturnClient};

const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";

const HTTP_PORT: u16 = 80;
use defmt::info;
// Todo: tftp client
#[cortex_m_rt::entry]

fn main() -> ! {
    if let Err(something) = connect_and_run(
        "Hello, HTTP server",
        ClientType::TcpFull,
        |stack: ReturnClient, ip: core::net::Ipv4Addr| -> Result<(), StackError> {
            if let ReturnClient::TcpFull(stack) = stack {
                defmt::info!(
                    "Starting HTTP server at http://{}.{}.{}.{}:{}",
                    ip.octets()[0],
                    ip.octets()[1],
                    ip.octets()[2],
                    ip.octets()[3],
                    HTTP_PORT
                );
                http_server::http_server(stack, HTTP_PORT)?;
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
