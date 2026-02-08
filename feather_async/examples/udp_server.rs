//! Async UDP server example - mirrors feather/examples/udp_server.rs
//!
//! Listens for incoming UDP packets on a specified port and responds
//! with "Hello, client_X!" where X is the last alphabetic character
//! from the received message (or 'x' as default).

#![no_std]
#![no_main]

use core::str::FromStr;
use embassy_time::Timer;
use embedded_nal_async::UnconnectedUdp;
use feather_async::hal::ehal::digital::OutputPin;
use feather_async::init::init;
use feather_async::shared::SpiStream;
use wincwifi::{AsyncClient, StackError};

const DEFAULT_TEST_PORT: &str = "12345";

async fn program() -> Result<(), StackError> {
    let ini = init().await.expect("Failed to initialize");
    defmt::info!("Async UDP server");

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

    // Parse configuration from environment
    let test_port = option_env!("TEST_PORT").unwrap_or(DEFAULT_TEST_PORT);
    let port = u16::from_str(test_port).unwrap_or(12345);
    let loop_forever = option_env!("LOOP_FOREVER").unwrap_or("false");
    let loop_forever = bool::from_str(loop_forever).unwrap_or(false);

    // Bind to UDP port (mirroring sync version's stack.bind())
    defmt::info!("-----Binding to UDP port {}-----", port);
    module.bind_udp(port).await?;
    defmt::info!("-----Bound to UDP port {}-----", port);

    // Main server loop
    loop {
        // Buffer size matches sync version (1500 bytes)
        let mut buf = [0u8; 1500];

        // Receive packet (mirroring sync version's stack.receive())
        match module.receive_into(&mut buf).await {
            Ok((n, local, remote)) => {
                let remote_port = remote.port();
                defmt::info!("-----Received {} bytes from port {}-----", n, remote_port);

                // Extract last alphabetic character as nonce (same logic as sync)
                let nonce = buf[..n]
                    .iter()
                    .rev()
                    .find(|&&c| c.is_ascii_alphabetic())
                    .copied()
                    .unwrap_or(b'x');

                // Build response with nonce: "Hello, client_X!" (same as sync)
                let mut response = *b"Hello, client_x!";
                response[14] = nonce;

                // Send response (mirroring sync version's stack.send_to())
                match module.send(local, remote, &response).await {
                    Ok(()) => {
                        defmt::info!("-----Sent response to port {}-----", remote_port);
                        // Blink LED on success
                        let _ = red_led.set_high();
                        Timer::after_millis(50).await;
                        let _ = red_led.set_low();
                    }
                    Err(e) => {
                        defmt::error!("Failed to send response: {:?}", e);
                    }
                }

                // Handle loop_forever flag (same as sync)
                if !loop_forever {
                    defmt::info!("Quitting the loop");
                    break;
                }
                defmt::info!("Looping again");
            }
            Err(e) => {
                defmt::error!("Receive error: {:?}", e);
                Timer::after_millis(100).await;
            }
        }
    }

    Ok(())
}

#[embassy_executor::main]
async fn main(_s: embassy_executor::Spawner) -> ! {
    match program().await {
        Ok(_) => defmt::info!("Good exit"),
        Err(e) => {
            defmt::error!("Something went wrong {:?}", e);
        }
    }

    loop {
        Timer::after_millis(1000).await;
    }
}
