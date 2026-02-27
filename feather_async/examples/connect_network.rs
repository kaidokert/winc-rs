#![no_std]
#![no_main]

use embassy_time::Timer;
use feather_async::hal::ehal::digital::OutputPin;
use feather_async::init::init;
use feather_async::shared::{AppError, SpiStream};
use wincwifi::{AsyncClient, Credentials, Ssid, StackError, WifiChannel};

const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";

async fn program() -> Result<(), AppError> {
    let ini = init().await?;

    defmt::info!("Hello, Winc Connect to Access Point Async Module");

    let mut red_led = ini.red_led;
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
    defmt::info!("Initializing module");
    module.start_wifi_module().await?;

    defmt::info!("Connecting to Access point...");
    module
        .connect_to_ap(&ssid, &credentials, WifiChannel::ChannelAll, false)
        .await?;
    defmt::info!("Connected to Access point...");

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
