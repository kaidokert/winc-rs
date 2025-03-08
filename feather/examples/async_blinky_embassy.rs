#![no_std]
#![no_main]

use core::convert::Infallible;

use feather::hal::ehal::digital::OutputPin;
use feather::init3::init;

use embassy_time::Timer;

async fn program() -> Result<(), Infallible> {
    if let Ok(ini) = init().await {
        defmt::info!("Embassy-time async blinky");
        let mut red_led = ini.red_led;
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
