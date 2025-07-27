//! Send a UDP packet to an IP address and port
//!

#![no_main]
#![no_std]

use bsp::shared::parse_ip_octets;
use core::str::FromStr;
use feather as bsp;
use feather::{error, info};
use wincwifi::StackError;

mod runner;
use runner::{connect_and_run, ClientType, ReturnClient};

use demos::udp_client;

const DEFAULT_TEST_IP: &str = "192.168.1.1";
const DEFAULT_TEST_PORT: &str = "12345";
const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";

#[cortex_m_rt::entry]
fn main() -> ! {
    if let Err(something) = connect_and_run(
        "Hello, UDP client",
        ClientType::Udp,
        |stack: ReturnClient, _: core::net::Ipv4Addr| -> Result<(), StackError> {
            if let ReturnClient::Udp(stack) = stack {
                info!("In UDP client stack thing");
                let test_ip = option_env!("TEST_IP").unwrap_or(DEFAULT_TEST_IP);
                let ip_values: [u8; 4] = parse_ip_octets(test_ip)?;
                let ip = core::net::Ipv4Addr::new(
                    ip_values[0],
                    ip_values[1],
                    ip_values[2],
                    ip_values[3],
                );
                let test_port = option_env!("TEST_PORT").unwrap_or(DEFAULT_TEST_PORT);
                let port = u16::from_str(test_port).unwrap_or(12345);
                info!("---- Starting UDP client ---- ");
                udp_client::udp_client(stack, ip, port)?;
                info!("---- HTTP UDP done ---- ");
            }
            Ok(())
        },
    ) {
        error!("Something went wrong {}", something);
    } else {
        info!("Good exit")
    };
    loop {}
}
