//! Async UDP server example - mirrors feather/examples/udp_server.rs
//!
//! Listens for incoming UDP packets on a specified port and responds
//! with "Hello, client_X!" where X is the last alphabetic character
//! from the received message (or 'x' as default).

#![no_std]
#![no_main]

use core::str::FromStr;
use embassy_time::Timer;
use feather_async::init::init;
use feather_async::shared::SpiStream;
use wincwifi::{AsyncClient, StackError};

const DEFAULT_TEST_PORT: &str = "12345";

async fn program() -> Result<(), StackError> {
    let ini = init().await.expect("Failed to initialize");
    defmt::info!("Async UDP server");

    let mut _red_led = ini.red_led;
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

    // Bind to UDP port - this is AsyncClient-specific setup
    defmt::info!("-----Binding to UDP port {}-----", port);
    module.bind_udp(port).await?;
    defmt::info!("-----Bound to UDP port {}-----", port);

    // Buffer size matches sync version (1500 bytes)
    let mut buf = [0u8; 1500];

    // Call generic UDP server with UnconnectedUdp trait
    demos_async::udp_server::run_udp_server(&mut module, port, loop_forever, &mut buf).await?;

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
