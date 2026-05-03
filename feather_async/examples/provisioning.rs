//! Provisioning Mode
//!

#![no_main]
#![no_std]

use embassy_time::Timer;
use feather_async::hal::ehal::digital::OutputPin;
use feather_async::init::init;
use feather_async::shared::{AppError, SpiStream};
use wincwifi::{AccessPoint, AsyncClient, HostName, Ssid, StackError, WifiChannel, WpaKey};

const DEFAULT_TEST_SSID: &str = "winc_network";
const DEFAULT_TEST_PASSWORD: &str = "password";
const DEFAULT_TEST_HOSTNAME: &str = "admin";
const DEFAULT_PROVISIONING_TIMEOUT_IN_MINS: u32 = 15;

async fn program() -> Result<(), AppError> {
    // init the feather board.
    let ini = init().await?;
    defmt::info!("Hello, Winc Provisioning");
    let mut red_led = ini.red_led;

    let ap_ssid = Ssid::from(option_env!("TEST_AP_SSID").unwrap_or(DEFAULT_TEST_SSID)).unwrap();
    let ap_password =
        WpaKey::from(option_env!("TEST_AP_PASSWORD").unwrap_or(DEFAULT_TEST_PASSWORD)).unwrap();
    let hostname =
        HostName::from(option_env!("TEST_AP_DNS").unwrap_or(DEFAULT_TEST_HOSTNAME)).unwrap();

    let mut stack = AsyncClient::new(SpiStream::new(ini.cs, ini.spi));

    defmt::info!("Initializing module");
    stack.start_wifi_module().await?;

    // Configure the access point with WPA/WPA2 security using the provided SSID and password.
    let access_point = AccessPoint::wpa(&ap_ssid, &ap_password);
    // Start the provisioning mode.
    defmt::info!(
        "Starting Provisioning Mode for {} minutes",
        DEFAULT_PROVISIONING_TIMEOUT_IN_MINS
    );
    let result = stack
        .start_provisioning_mode(
            &access_point,
            &hostname,
            true,
            DEFAULT_PROVISIONING_TIMEOUT_IN_MINS,
        )
        .await;

    // Check for provisioning information is received for 15 minutes.
    match result {
        Ok(info) => {
            defmt::info!("Credentials received from provisioning; connecting to access point.");
            // Connect to access point.
            stack
                .connect_to_ap(&info.ssid, &info.key, WifiChannel::ChannelAll, false)
                .await?;
            defmt::info!("Connected to AP");
        }
        Err(err) => {
            if err == StackError::GeneralTimeout {
                defmt::error!(
                    "No information was received for {} minutes. Stopping provisioning mode.",
                    DEFAULT_PROVISIONING_TIMEOUT_IN_MINS
                );
                stack.stop_provisioning_mode()?;
            } else {
                defmt::error!("Provisioning Failed");
            }
        }
    }

    loop {
        Timer::after_millis(200).await;
        red_led.set_high().unwrap();
        Timer::after_millis(200).await;
        red_led.set_low().unwrap();
        stack.heartbeat()?;
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
