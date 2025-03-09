//! Demonstrates the OLED display
//!
//! https://www.adafruit.com/product/2900
//!
#![no_main]
#![no_std]

use bsp::hal::prelude::*;
use feather as bsp;

use bsp::hal::ehal as embedded_hal;
use bsp::hal::ehal::{digital::InputPin, digital::OutputPin};

use feather::init::init;

use core::fmt::Write;
use ssd1306::mode::DisplayConfig;
use ssd1306::rotation::DisplayRotation;
use ssd1306::size::DisplaySize128x32;
use ssd1306::I2CDisplayInterface;
use ssd1306::Ssd1306;

use embedded_hal::digital::ErrorKind;

#[derive(Debug, defmt::Format)]
pub enum MyError {
    PinError(ErrorKind),
    DisplayError,
}

impl<T: embedded_hal::digital::Error> From<T> for MyError {
    fn from(e: T) -> Self {
        MyError::PinError(e.kind())
    }
}

fn program() -> Result<(), MyError> {
    if let Ok(ini) = init() {
        defmt::println!("Hello, OLED!");
        let interface = I2CDisplayInterface::new(ini.i2c);

        let mut display = Ssd1306::new(interface, DisplaySize128x32, DisplayRotation::Rotate0)
            .into_terminal_mode();
        display.init().map_err(|_| MyError::DisplayError)?;
        display.clear().map_err(|_| MyError::DisplayError)?;

        let mut delay = ini.delay_tick;
        let mut red_led = ini.red_led;
        let mut btn_a = ini.button_a;
        let mut btn_b = ini.button_b;
        let mut btn_c = ini.button_c;

        write!(display, "Hello, {}", "world").map_err(|_| MyError::DisplayError)?;

        let mut btn_states = [btn_a.is_low()?, btn_b.is_low()?, btn_c.is_low()?];

        loop {
            delay.delay_ms(200u32);
            red_led.set_high()?;
            delay.delay_ms(200u32);
            red_led.set_low()?;

            let new_btn_states = [btn_a.is_low()?, btn_b.is_low()?, btn_c.is_low()?];
            if new_btn_states != btn_states {
                btn_states = new_btn_states;
                defmt::println!(
                    "Button states: A: {:?}, B: {:?}, C: {:?}",
                    btn_states[0],
                    btn_states[1],
                    btn_states[2]
                );
                display.clear().map_err(|_| MyError::DisplayError)?;
                write!(
                    display,
                    "A:{} B:{} C:{}",
                    if btn_states[0] { "X" } else { "O" },
                    if btn_states[1] { "X" } else { "O" },
                    if btn_states[2] { "X" } else { "O" }
                )
                .map_err(|_| MyError::DisplayError)?;
            }
        }
    } else {
        panic!("Failed to initialize");
    }
}

#[cortex_m_rt::entry]
fn main() -> ! {
    if let Err(err) = program() {
        defmt::error!("Error: {:?}", err);
        panic!("Error in main program");
    } else {
        defmt::info!("Good exit")
    };
    loop {}
}
