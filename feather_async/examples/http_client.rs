#![no_std]
#![no_main]

use core::net::Ipv4Addr;
use core::str::FromStr;
use embassy_time::Timer;
use feather_async::hal::ehal::digital::OutputPin;
use feather_async::init::init;
use feather_async::shared::SpiStream;
use wincwifi::{AsyncClient, StackError};

const DEFAULT_TEST_IP: &str = "192.168.1.100";
const DEFAULT_TEST_PORT: &str = "80";

async fn program() -> Result<(), StackError> {
    let ini = init().await.map_err(|_| StackError::Unexpected)?;

    defmt::info!("Embassy-time async Http client");
    let mut red_led = ini.red_led;
    let mut module = AsyncClient::new(SpiStream::new(ini.cs, ini.spi));

    defmt::info!("Initializing module");
    module.start_wifi_module().await?;

    // Give network time to stabilize
    for _ in 0..20 {
        Timer::after_millis(100).await;
        let _ = module.heartbeat();
    }

    defmt::info!("Connecting to saved network");
    module.connect_to_saved_ap().await?;
    defmt::info!("Connected to saved network");

    // Parse server configuration
    let server_ip_str = option_env!("TEST_IP").unwrap_or(DEFAULT_TEST_IP);
    let server_port_str = option_env!("TEST_PORT").unwrap_or(DEFAULT_TEST_PORT);
    let test_host = option_env!("TEST_HOST");

    let server_ip = Ipv4Addr::from_str(server_ip_str).map_err(|_| StackError::InvalidParameters)?;
    let server_port = u16::from_str(server_port_str).unwrap_or(12345);

    defmt::info!(
        "Server configured: {}.{}.{}.{}:{}",
        server_ip.octets()[0],
        server_ip.octets()[1],
        server_ip.octets()[2],
        server_ip.octets()[3],
        server_port
    );

    defmt::info!("---- Starting HTTP client ---- ");
    demos_async::http_client::run_http_client(&mut module, server_ip, server_port, test_host)
        .await?;
    defmt::info!("---- HTTP Client done ---- ");

    loop {
        Timer::after_millis(200).await;
        red_led.set_high().unwrap();
        Timer::after_millis(200).await;
        red_led.set_low().unwrap();
    }
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
