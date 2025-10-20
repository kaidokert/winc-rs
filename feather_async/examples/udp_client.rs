//! Async UDP client example
//!
//! Sends a UDP packet to a server and receives a response.
//! Configure server IP and port via environment variables:
//!   export TEST_SERVER_IP=192.168.1.100
//!   export TEST_SERVER_PORT=12345
//!

#![no_std]
#![no_main]

use core::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use core::str::FromStr;
use embassy_time::Timer;
use feather_async::hal::ehal::digital::OutputPin;
use feather_async::init::init;
use feather_async::shared::SpiStream;
use wincwifi::{AsyncClient, StackError};

const DEFAULT_TEST_SERVER_IP: &str = "192.168.1.100";
const DEFAULT_TEST_SERVER_PORT: &str = "12345";
const DEFAULT_LOCAL_PORT: u16 = 0; // Auto-assign local port

async fn program() -> Result<(), StackError> {
    if let Ok(ini) = init().await {
        defmt::info!("Embassy-time async UDP client");
        let mut red_led = ini.red_led;
        let mut module = AsyncClient::new(SpiStream::new(ini.cs, ini.spi));

        defmt::info!("Initializing module");
        module.start_wifi_module().await?;

        defmt::info!("Connecting to saved network");
        module.connect_to_saved_ap().await?;
        defmt::info!("Connected to saved network");

        // Give network time to stabilize
        for _ in 0..20 {
            Timer::after_millis(100).await;
            let _ = module.heartbeat();
        }

        // Parse server configuration
        let server_ip_str = option_env!("TEST_SERVER_IP").unwrap_or(DEFAULT_TEST_SERVER_IP);
        let server_port_str = option_env!("TEST_SERVER_PORT").unwrap_or(DEFAULT_TEST_SERVER_PORT);

        let server_ip = parse_ipv4(server_ip_str).ok_or(StackError::InvalidParameters)?;
        let server_port = u16::from_str(server_port_str).unwrap_or(12345);

        defmt::info!(
            "Server configured: {}.{}.{}.{}:{}",
            server_ip.octets()[0],
            server_ip.octets()[1],
            server_ip.octets()[2],
            server_ip.octets()[3],
            server_port
        );

        // Prepare local and server addresses
        let local_addr =
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, DEFAULT_LOCAL_PORT));

        // Prepare test data
        let test_data = b"Hello from async UDP client!";
        let mut recv_buffer = [0u8; 1024];

        defmt::info!("Sending UDP packet...");

        let recv_len = demos_async::udp_client::run_udp_client(
            &mut module,
            local_addr,
            server_ip,
            server_port,
            test_data,
            &mut recv_buffer,
        )
        .await?;

        defmt::info!("Received {} bytes", recv_len);

        // Blink LED to indicate completion
        loop {
            Timer::after_millis(200).await;
            red_led.set_high().unwrap();
            Timer::after_millis(200).await;
            red_led.set_low().unwrap();
        }
    }
    Ok(())
}

/// Parse IPv4 address from string (no_std compatible)
fn parse_ipv4(s: &str) -> Option<Ipv4Addr> {
    let mut octets = [0u8; 4];
    let mut octet_idx = 0;
    let mut current_num = 0u16;
    let mut has_digit = false;

    for c in s.chars() {
        if c == '.' {
            if !has_digit || current_num > 255 || octet_idx >= 3 {
                return None;
            }
            octets[octet_idx] = current_num as u8;
            octet_idx += 1;
            current_num = 0;
            has_digit = false;
        } else if c.is_ascii_digit() {
            current_num = current_num * 10 + (c as u16 - '0' as u16);
            if current_num > 255 {
                return None;
            }
            has_digit = true;
        } else {
            return None;
        }
    }

    // Handle last octet
    if !has_digit || current_num > 255 || octet_idx != 3 {
        return None;
    }
    octets[octet_idx] = current_num as u8;

    Some(Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3]))
}

#[embassy_executor::main]
async fn main(_s: embassy_executor::Spawner) -> ! {
    if let Err(err) = program().await {
        defmt::error!("Error: {}", err);
        panic!("Error in main program");
    } else {
        defmt::info!("Good exit")
    };
    loop {}
}
