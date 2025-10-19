//! A simple UDP server that listens for incoming packets

#![no_main]
#![no_std]
#![allow(unused_imports)]

use embedded_nal::UdpFullStack;

use feather as bsp;
use feather::{error, info};
mod runner;
use core::str::FromStr;

use demos::udp_server;

use runner::{connect_and_run, ClientType, ReturnClient};
use wincwifi::StackError;

const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";
const DEFAULT_TEST_PORT: &str = "12345";

// Todo: UDP server
#[cortex_m_rt::entry]
fn main() -> ! {
    if let Err(something) = connect_and_run(
        "Hello, UDP server",
        ClientType::UdpFull,
        |stack: ReturnClient, my_ip: core::net::Ipv4Addr| -> Result<(), StackError> {
            if let ReturnClient::UdpFull(stack) = stack {
                let test_port = option_env!("TEST_PORT").unwrap_or(DEFAULT_TEST_PORT);
                let port = u16::from_str(test_port).unwrap_or(12345);
                let loop_forever = option_env!("LOOP_FOREVER").unwrap_or("false");
                let loop_forever = bool::from_str(loop_forever).unwrap_or(false);
                // Format IP as octets for defmt compatibility
                let octets = my_ip.octets();
                info!(
                    "Starting UDP server at IP: {}.{}.{}.{} port: {}",
                    octets[0], octets[1], octets[2], octets[3], port
                );
                udp_server::udp_server(stack, port, loop_forever)?;
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
