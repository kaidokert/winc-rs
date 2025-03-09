//! A mandatory blinky to verify the board is working

#![no_main]
#![no_std]

use bsp::hal::ehal::digital::OutputPin;
use bsp::hal::prelude::*;
use feather as bsp;
use feather::init2::init;

#[cortex_m_rt::entry]
fn main() -> ! {
    if let Ok(mut ini) = init() {
        let delay = &mut ini.delay_tick;
        let red_led = &mut ini.red_led;
        defmt::println!("Hello, blinky!");
        loop {
            delay.delay_ms(200u32);
            red_led.set_high().unwrap();
            delay.delay_ms(200u32);
            red_led.set_low().unwrap();
        }
    } else {
        panic!("Failed to initialize");
    }
}
