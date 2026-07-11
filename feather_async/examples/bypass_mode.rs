//! Ethernet bypass mode example

#![no_std]
#![no_main]

use core::net::Ipv4Addr;
use core::str::FromStr;
use embassy_executor::Spawner;
use embassy_net::icmp::{
    ping::{PingManager, PingParams},
    PacketMetadata,
};
use embassy_net::{Config, Stack, StackResources};
use embassy_time::{Duration, Instant, Timer};
use feather_async::{
    init::init,
    shared::{AppError, SpiStream},
};
use static_cell::StaticCell;
use wincwifi::{AsyncClient, Credentials, Ssid, StackError, WifiChannel};

// Default values for the example
const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";
const DEFAULT_TEST_IP: &str = "8.8.8.8";
const DEFAULT_TEST_COUNT: &str = "4";
// Maximum number of bytes in IP address
const MAX_IP_BYTES: usize = 4;
// Max storage size for ICMP storage.
const MAX_ICMP_STORAGE_SIZE: usize = 256;
// Timeout before sending a new Ping packet
const PING_PACKET_TIMEOUT: u64 = 500;
// Custom payload for the Ping packet
const PING_PACKET_PAYLOAD: &[u8] = b"Hello, Ping!";
// Maximum bytes in the random seed
const MAX_RANDOM_SEED_BYTES: usize = 8;
// Resources for the network stack
const RESOURCE_CAPACITY: usize = 3;
// Timeout for DHCP
const DHCP_TIMEOUT: Duration = Duration::from_secs(30);
// Poll interval for DHCP
const DHCP_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Static cell for storing network stack resources.
static RESOURCES: StaticCell<StackResources<RESOURCE_CAPACITY>> = StaticCell::new();

/// Waits for DHCP to complete and returns the static configuration.
///
/// # Arguments
///
/// * `stack` - The network stack instance.
///
/// # Returns
///
/// * `embassy_net::StaticConfigV4` - The static configuration for the network.
async fn wait_for_dhcp(stack: &Stack<'static>) -> Result<(), AppError> {
    let timeout = Instant::now() + DHCP_TIMEOUT;
    defmt::info!("Waiting for DHCP...");

    loop {
        if let Some(config) = stack.config_v4() {
            let ip: [u8; MAX_IP_BYTES] = config.address.address().octets();
            let gateway: [u8; MAX_IP_BYTES] = config
                .gateway
                .ok_or(AppError::WincError(StackError::InvalidResponse))?
                .octets();
            let mask: [u8; MAX_IP_BYTES] = config.address.netmask().octets();
            defmt::info!("IP address: {}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]);
            defmt::info!(
                "Gateway address: {}.{}.{}.{}",
                gateway[0],
                gateway[1],
                gateway[2],
                gateway[3]
            );
            defmt::info!(
                "DNS server: {}.{}.{}.{}",
                mask[0],
                mask[1],
                mask[2],
                mask[3]
            );
            return Ok(());
        }
        if Instant::now() > timeout {
            defmt::error!("DHCP timeout after {}s", DHCP_TIMEOUT.as_secs());
            return Err(AppError::WincError(StackError::GeneralTimeout));
        }

        Timer::after(DHCP_POLL_INTERVAL).await;
    }
}

/// Pings the specified IP address using the ICMP protocol.
///
/// # Arguments
///
/// * `stack` - The network stack instance.
/// * `destination_ip` - The IP address to ping.
/// * `count` - The number of pings to send.
#[embassy_executor::task]
async fn ping_task(stack: Stack<'static>, destination_ip: Ipv4Addr, count: u16) -> ! {
    // Wait for DHCP
    if let Err(err) = wait_for_dhcp(&stack).await {
        defmt::error!("DHCP error: {:?}", err);
        loop {
            Timer::after(Duration::from_secs(1)).await
        }
    }

    // Then we can use it!
    let mut rx_buffer = [0; MAX_ICMP_STORAGE_SIZE];
    let mut tx_buffer = [0; MAX_ICMP_STORAGE_SIZE];
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
    ping_params.set_payload(PING_PACKET_PAYLOAD); // custom payload
    ping_params.set_count(count);
    ping_params.set_timeout(Duration::from_millis(PING_PACKET_TIMEOUT));

    // Execute the ping with the given parameters and wait for the reply
    match ping_manager.ping(&ping_params).await {
        Ok(time) => {
            let ip_octets = destination_ip.octets();
            defmt::info!(
                "Ping from {}.{}.{}.{} succeeded, latency: {} ms",
                ip_octets[0],
                ip_octets[1],
                ip_octets[2],
                ip_octets[3],
                time.as_millis()
            );
        }
        Err(_) => {
            defmt::error!("Ping failed");
        }
    }

    loop {
        embassy_time::Timer::after_secs(3600).await
    }
}

/// Main Program
async fn program(spwaner: Spawner) -> Result<(), AppError> {
    let ini = init().await?;

    defmt::info!("Hello, Winc Async Embassy Net Ping Example");

    let test_ip = option_env!("TEST_IP").unwrap_or(DEFAULT_TEST_IP);
    let test_ip: Ipv4Addr =
        Ipv4Addr::from_str(test_ip).map_err(|_| StackError::InvalidParameters)?;
    let test_count = option_env!("TEST_COUNT").unwrap_or(DEFAULT_TEST_COUNT);
    let test_count = u16::from_str(test_count).map_err(|_| StackError::InvalidParameters)?;

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
    let mut random_bytes = [0u8; MAX_RANDOM_SEED_BYTES];
    module.get_random_bytes(&mut random_bytes).await?;
    let seed = u64::from_le_bytes(random_bytes);

    // Init network
    let resources = RESOURCES.init(StackResources::<RESOURCE_CAPACITY>::new());
    let (stack, mut runner) =
        embassy_net::new(module, Config::dhcpv4(Default::default()), resources, seed);

    // Start the ping task
    spwaner
        .spawn(ping_task(stack, test_ip, test_count))
        .expect("Failed to spawn ping task");

    runner.run().await;
}

/// The main program entry point.
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
