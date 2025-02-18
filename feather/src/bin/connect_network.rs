#![no_main]
#![no_std]

use bsp::hal::prelude::*;
use bsp::shared::{create_delay_closure, SpiStream};
use feather as bsp;
use feather::init::init;

use cortex_m_systick_countdown::MillisCountDown;

const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";

use wincwifi::{StackError, WincClient};

fn program() -> Result<(), StackError> {
    if let Ok((delay_tick, mut red_led, cs, spi)) = init() {
        defmt::println!("Hello, Winc Module");

        let mut countdown1 = MillisCountDown::new(&delay_tick);
        let mut countdown2 = MillisCountDown::new(&delay_tick);
        let mut countdown3 = MillisCountDown::new(&delay_tick);
        let mut delay_ms = create_delay_closure(&mut countdown1);
        let mut delay_ms2 = create_delay_closure(&mut countdown2);

        let ssid = option_env!("TEST_SSID").unwrap_or(DEFAULT_TEST_SSID);
        let password = option_env!("TEST_PASSWORD").unwrap_or(DEFAULT_TEST_PASSWORD);
        defmt::info!(
            "Connecting to network: {} with password: {}",
            ssid,
            password
        );
        let mut stack = WincClient::new(
            SpiStream::new(cs, spi, create_delay_closure(&mut countdown3)),
            &mut delay_ms2,
        );

        stack
            .start_module(&mut |v: u32| -> bool {
                defmt::debug!("Waiting start .. {}", v);
                delay_ms(20);
                false
            })
            .unwrap();

        defmt::info!("Started, connecting to AP ..");
        nb::block!(stack.connect_to_ap(ssid, password))?;

        defmt::info!(".. connected to AP, going to loop");
        loop {
            delay_ms(200);
            red_led.set_high()?;
            delay_ms(200);
            red_led.set_low()?;
            stack.heartbeat().unwrap();
        }
    }
    Ok(())
}

#[cortex_m_rt::entry]
fn main() -> ! {
    if let Err(err) = program() {
        defmt::info!("Bad error {}", err);
        panic!("Error in main program");
    } else {
        defmt::info!("Good exit")
    };
    loop {}
}
