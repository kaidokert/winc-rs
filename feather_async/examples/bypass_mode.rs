//! Ethernet bypass mode example

#![no_std]
#![no_main]

use core::net::Ipv4Addr;
use core::str::FromStr;

use embassy_executor::Spawner;
use embassy_futures::yield_now;
use embassy_net::icmp::ping::{PingManager, PingParams};
use embassy_net::icmp::PacketMetadata;
use embassy_net::{Config, Stack, StackResources};
use embassy_time::Duration;
use feather_async::init::init;
use feather_async::shared::{AppError, SpiStream};
use static_cell::StaticCell;
use wincwifi::{AsyncClient, Credentials, Ssid, StackError, WifiChannel};

const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";
const DEFAULT_TEST_IP: &str = "8.8.8.8";
const DEFAULT_TEST_COUNT: &str = "4";

static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();

async fn wait_for_dhcp(stack: Stack<'static>) -> embassy_net::StaticConfigV4 {
    defmt::info!("Waiting for DHCP...");
    loop {
        if let Some(config) = stack.config_v4() {
            return config.clone();
        }
        yield_now().await;
    }
}

#[embassy_executor::task]
async fn ping_task(stack: Stack<'static>, destination_ip: Ipv4Addr, count: u16) -> ! {
    // Wait for DHCP
    let cfg = wait_for_dhcp(stack).await;
    let local_addr = cfg.address.address().octets();
    defmt::info!(
        "IP address: {}.{}.{}.{}",
        local_addr[0],
        local_addr[1],
        local_addr[2],
        local_addr[3]
    );

    // Then we can use it!
    let mut rx_buffer = [0; 256];
    let mut tx_buffer = [0; 256];
    let mut rx_meta = [PacketMetadata::EMPTY];
    let mut tx_meta = [PacketMetadata::EMPTY];

    // Create the ping manager instance
    let mut ping_manager = PingManager::new(
        stack,
        &mut rx_meta,
        &mut rx_buffer,
        &mut tx_meta,
        &mut tx_buffer,
    );

    // Create the PingParams with the destination address
    let mut ping_params = PingParams::new(destination_ip);
    // (optional) Set custom properties of the ping
    ping_params.set_payload(b"Hello, Ping!"); // custom payload
    ping_params.set_count(count); // ping 1 times per ping call
    ping_params.set_timeout(Duration::from_millis(500)); // wait .5 seconds instead of 4

    // Execute the ping with the given parameters and wait for the reply
    match ping_manager.ping(&ping_params).await {
        Ok(time) => {
            let ip_octets = destination_ip.octets();
            defmt::info!(
                "Ping {}.{}.{}.{} succeeded, latency: {} ms",
                ip_octets[0],
                ip_octets[1],
                ip_octets[2],
                ip_octets[3],
                time.as_millis()
            );
        }
        Err(_) => {
            // Todo add error handling
            defmt::error!("Ping failed");
        }
    }

    defmt::info!("Ping task complete");

    loop {}
}

async fn program(spwaner: Spawner) -> Result<(), AppError> {
    let ini = init().await?;

    defmt::info!("Hello, Winc Async Embassy Net Ping Example");

    let test_ip = option_env!("TEST_IP").unwrap_or(DEFAULT_TEST_IP);
    let test_ip: Ipv4Addr =
        Ipv4Addr::from_str(test_ip).map_err(|_| StackError::InvalidParameters)?;
    let test_count = option_env!("TEST_COUNT").unwrap_or(DEFAULT_TEST_COUNT);
    let test_count = u16::from_str(test_count).unwrap();

    let ssid = Ssid::from(option_env!("TEST_SSID").unwrap_or(DEFAULT_TEST_SSID))
        .map_err(|_| StackError::InvalidParameters)?;
    let password = option_env!("TEST_PASSWORD").unwrap_or(DEFAULT_TEST_PASSWORD);
    let credentials = Credentials::from_wpa(password)?;

    defmt::info!(
        "Connecting to network: {} with password: {}",
        ssid.as_str(),
        password
    );

    let mut module = AsyncClient::new(SpiStream::new(ini.cs, ini.spi));
    defmt::info!("Initializing module in ethernet mode");
    module.start_in_ethernet_mode().await?;

    defmt::info!("Module initialized in ethernet mode");

    defmt::info!("Connecting to Access point...");
    module
        .connect_to_ap(&ssid, &credentials, WifiChannel::ChannelAll, false)
        .await?;
    defmt::info!("Connected to Access point...");

    // Generate seed
    let mut random_bytes = [0u8; 8];
    module.get_random_bytes(&mut random_bytes).await?;
    let seed = u64::from_le_bytes(random_bytes);

    // Init network
    let resources = RESOURCES.init(StackResources::<3>::new());
    let (stack, mut runner) =
        embassy_net::new(module, Config::dhcpv4(Default::default()), resources, seed);

    // Start the ping task
    spwaner
        .spawn(ping_task(stack, test_ip, test_count))
        .expect("Failed to spawn ping task");

    runner.run().await;
}

#[embassy_executor::main]
async fn main(s: embassy_executor::Spawner) -> ! {
    if let Err(err) = program(s).await {
        defmt::error!("Error: {}", err);
        panic!("Error in main program");
    } else {
        defmt::info!("Good exit")
    };
    loop {}
}
