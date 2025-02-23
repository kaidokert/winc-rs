//! A mandatory blinky to verify the board is working

#![no_main]
#![no_std]

use bsp::hal::prelude::*;
use feather as bsp;

use feather::init::init;

#[cortex_m_rt::entry]
fn main() -> ! {
    if let Ok((mut delay, mut red_led, _cs, _spi)) = init() {
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
