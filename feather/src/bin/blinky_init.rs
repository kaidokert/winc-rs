#![no_main]
#![no_std]

use feather as bsp;
use bsp::hal::prelude::*;

use feather::init::init;

use cortex_m_systick_countdown::{PollingSysTick, SysTickCalibration};

#[cortex_m_rt::entry]
fn main() -> ! {
    if let Ok((mut delay, mut red_led)) = init() {
        defmt::println!("Hello, blinky with shared init!");
        loop {
            delay.delay_ms(200u32);
            red_led.set_high().unwrap();
            delay.delay_ms(200u32);
            red_led.set_low().unwrap();
        }
    }
    loop {}
}