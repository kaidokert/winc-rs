#![no_main]
#![no_std]

use feather as _;

use feather::bsp;
use bsp::{hal,pac};
use pac::{CorePeripherals, Peripherals};
use bsp::pin_alias;
use hal::clock::GenericClockController;
use hal::delay::Delay;
use hal::prelude::*;

#[cortex_m_rt::entry]
fn main() -> ! {
    let mut peripherals = Peripherals::take().unwrap();
    let core = CorePeripherals::take().unwrap();

    let mut clocks = GenericClockController::with_internal_32kosc(
        peripherals.GCLK,
        &mut peripherals.PM,
        &mut peripherals.SYSCTRL,
        &mut peripherals.NVMCTRL,
    );
    let pins = bsp::Pins::new(peripherals.PORT);
    let mut red_led: bsp::RedLed = pin_alias!(pins.red_led).into();
    let mut delay = Delay::new(core.SYST, &mut clocks);

    defmt::println!("Hello, defmt!");
    loop {
        delay.delay_ms(200u8);
        red_led.set_high().unwrap();
        delay.delay_ms(200u8);
        red_led.set_low().unwrap();
    }
}