#![no_main]
#![no_std]
#![allow(unused_imports)]

use core::net::IpAddr;

use feather as bsp;

use embedded_nal::Dns;
use wincwifi::StackError;
mod runner;
use runner::{connect_and_run, ClientType, MyDns, ReturnClient};

const DEFAULT_TEST_HOST: &str = "www.google.com";
const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";

fn dns_client<T>(stack: &mut T, host: &str) -> Result<(), T::Error>
where
    T: Dns + ?Sized,
    T::Error: From<embedded_nal::nb::Error<T::Error>>,
{
    let ip = nb::block!(stack.get_host_by_name(host, embedded_nal::AddrType::IPv4));
    match ip {
        Ok(IpAddr::V4(ip)) => {
            let octets = ip.octets();
            defmt::println!(
                "DNS: {} -> {}.{}.{}.{}",
                host,
                octets[0],
                octets[1],
                octets[2],
                octets[3]
            )
        }
        _ => defmt::error!("DNS failed: {}", host),
    }

    Ok(())
}

// Todo: DNS lookups
#[cortex_m_rt::entry]
fn main() -> ! {
    if let Err(something) = connect_and_run(
        "Hello, DNS client",
        ClientType::Dns,
        |stack: ReturnClient, _: core::net::Ipv4Addr| -> Result<(), StackError> {
            if let ReturnClient::Dns(stack) = stack {
                let host = option_env!("TEST_HOST").unwrap_or(DEFAULT_TEST_HOST);
                dns_client(stack, host)?;
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
