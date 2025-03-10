#![no_std]
#![no_main]

use embassy_time::Timer;
use feather_async::hal::ehal::digital::OutputPin;
use feather_async::init::init;
use feather_async::shared::SpiStream;
use wincwifi::{AsyncClient, StackError};

async fn program() -> Result<(), StackError> {
    if let Ok(ini) = init().await {
        defmt::info!("Embassy async blinky");
        let mut red_led = ini.red_led;
        let mut module = AsyncClient::new(SpiStream::new(ini.cs, ini.spi));
        defmt::info!("Initializing module");
        module.start_wifi_module().await?;
        defmt::info!("Connecting to saved network");
        module.connect_to_saved_ap().await?;
        defmt::info!("Connected to saved network");
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
