#![no_std]
#![no_main]

use cortex_m_systick_countdown::MillisCountDown;
use feather::hal::ehal::digital::OutputPin;
use feather::init2::init;
use feather::shared::delay_fn;
use feather::shared::SpiStream;
use wincwifi::AsyncClient;
use wincwifi::StackError;

async fn program() -> Result<(), StackError> {
    if let Ok(ini) = init() {
        defmt::info!("Embassy async blinky");
        let mut red_led = ini.red_led;
        let mut cnt = MillisCountDown::new(&ini.delay_tick);
        let mut delay = delay_fn(&mut cnt);
        let mut module = AsyncClient::new(SpiStream::new(ini.cs, ini.spi));
        defmt::info!("Initializing module");
        module.start_wifi_module().await?;
        defmt::info!("Connecting to saved network");
        module.connect_to_saved_ap().await?;
        defmt::info!("Connected to saved network");
        loop {
            delay(200u32); // Todo: replace this with embbassy_time::Timer::after_millis(200).await
            red_led.set_high().unwrap();
            delay(200u32); // Todo: replace this with embbassy_time::Timer::after_millis(200).await
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
