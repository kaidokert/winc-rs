//! Demonstrate async DNS lookup
//!
//! Input is a single or a comma separated list of hosts
//! export TEST_HOST=www.google.com,www.rustinaction.com
//!

#![no_std]
#![no_main]

use embedded_nal_async::Dns;

use core::net::IpAddr;
use embassy_time::Timer;
use feather_async::hal::ehal::digital::OutputPin;
use feather_async::init::init;
use feather_async::shared::SpiStream;
use wincwifi::{AsyncClient, StackError};

const DEFAULT_TEST_HOST: &str = "www.google.com";

// Todo: move this to demos_async
async fn dns_client<T>(stack: &mut T, host: &str) -> Result<(), T::Error>
where
    T: Dns + ?Sized,
    T::Error: core::fmt::Debug + defmt::Format,
{
    defmt::info!("DNS lookup for: {}", host);
    let ip = stack
        .get_host_by_name(host, embedded_nal_async::AddrType::IPv4)
        .await;
    match ip {
        Ok(IpAddr::V4(ip)) => defmt::info!(
            "DNS: {} -> {}.{}.{}.{}",
            host,
            ip.octets()[0],
            ip.octets()[1],
            ip.octets()[2],
            ip.octets()[3]
        ),
        Err(e) => defmt::error!("DNS: {:?} -> {:?}", host, e),
        _ => defmt::error!("DNS: {:?} -> {:?}", host, "No IP address found"),
    }
    Ok(())
}

async fn program() -> Result<(), StackError> {
    if let Ok(ini) = init().await {
        defmt::info!("Embassy-time async DNS");
        let mut red_led = ini.red_led;
        let mut module = AsyncClient::new(SpiStream::new(ini.cs, ini.spi));
        defmt::info!("Initializing module");
        module.start_wifi_module().await?;
        defmt::info!("Connecting to saved network");
        module.connect_to_saved_ap().await?;
        defmt::info!("Connected to saved network");
        for _ in 0..20 {
            Timer::after_millis(100).await;
            let _ = module.heartbeat();
        }
        let host = option_env!("TEST_HOST").unwrap_or(DEFAULT_TEST_HOST);
        for host_part in host.split(',') {
            dns_client(&mut module, host_part).await?;
        }
        loop {
            Timer::after_millis(200).await;
            red_led.set_high().unwrap();
            Timer::after_millis(200).await;
            red_led.set_low().unwrap();
        }
    }
    Ok(())
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
