#![no_main]
#![no_std]

use bsp::shared::parse_ip_octets;
use core::str::FromStr;
use feather as bsp;
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
        |stack: ReturnClient| -> Result<(), StackError> {
            if let ReturnClient::Udp(stack) = stack {
                defmt::info!("In UDP client stack thing");
                let test_ip = option_env!("TEST_IP").unwrap_or(DEFAULT_TEST_IP);
                let ip_values: [u8; 4] = parse_ip_octets(test_ip);
                let ip = core::net::Ipv4Addr::new(
                    ip_values[0],
                    ip_values[1],
                    ip_values[2],
                    ip_values[3],
                );
                let test_port = option_env!("TEST_PORT").unwrap_or(DEFAULT_TEST_PORT);
                let port = u16::from_str(test_port).unwrap_or(12345);
                defmt::info!("---- Starting UDP client ---- ");
                udp_client::udp_client(stack, ip, port)?;
                defmt::info!("---- HTTP UDP done ---- ");
            }
            Ok(())
        },
    ) {
        defmt::info!("Something went wrong {}", something)
    } else {
        defmt::info!("Good exit")
    };
    loop {}
}
