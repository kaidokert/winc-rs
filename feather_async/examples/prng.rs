//! Async deterministic pseudo-random number generator (PRNG)

#![no_std]
#![no_main]

use embassy_time::Timer;
use feather_async::hal::ehal::digital::OutputPin;
use feather_async::init::init;
use feather_async::shared::{AppError, SpiStream};
use wincwifi::AsyncClient;

async fn program() -> Result<(), AppError> {
    let ini = init().await?;

    defmt::info!("Hello, Winc Async PRNG");

    let mut red_led = ini.red_led;

    let mut stack = AsyncClient::new(SpiStream::new(ini.cs, ini.spi));
    defmt::info!("Initializing module");
    stack.start_wifi_module().await?;

    let mut random_bytes: [u8; 32] = [0; 32];
    stack.get_random_bytes(&mut random_bytes).await?;
    defmt::info!("Got the Random bytes: {}", random_bytes);

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
