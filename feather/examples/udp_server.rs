//! A simple UDP server that listens for incoming packets

#![no_main]
#![no_std]
#![allow(unused_imports)]

use embedded_nal::UdpFullStack;

use feather as bsp;
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
        |stack: ReturnClient, _: core::net::Ipv4Addr| -> Result<(), StackError> {
            if let ReturnClient::UdpFull(stack) = stack {
                let test_port = option_env!("TEST_PORT").unwrap_or(DEFAULT_TEST_PORT);
                let port = u16::from_str(test_port).unwrap_or(12345);
                let loop_forever = option_env!("LOOP_FOREVER").unwrap_or("0");
                let loop_forever = bool::from_str(loop_forever).unwrap_or(false);
                udp_server::udp_server(stack, port, loop_forever)?;
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
