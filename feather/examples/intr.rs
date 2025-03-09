//! A mandatory blinky to verify the board is working

#![no_main]
#![no_std]

use core::sync::atomic::{AtomicUsize, Ordering};

use bsp::hal::ehal::digital::OutputPin;
use bsp::hal::prelude::*;
use feather as bsp;

use feather::bsp::pac;
use pac::interrupt;

use feather::init::init;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

#[cortex_m_rt::entry]
fn main() -> ! {
    if let Ok(mut ini) = init() {
        let delay = &mut ini.delay_tick;
        let red_led = &mut ini.red_led;
        defmt::println!("Hello, IRQs!");
        let mut prev_value: usize = 0;
        loop {
            delay.delay_ms(200u32);
            red_led.set_high().unwrap();
            delay.delay_ms(200u32);
            red_led.set_low().unwrap();
            let new_value = COUNTER.load(Ordering::SeqCst);
            if new_value != prev_value {
                prev_value = new_value;
                defmt::println!("Counter: {}", new_value);
            }
        }
    } else {
        panic!("Failed to initialize");
    }
}

#[interrupt]
fn EIC() {
    unsafe {
        // Accessing registers from interrupts context is safe
        let eic = &*pac::Eic::ptr();

        eic.intflag().modify(|_, w| w.extint5().set_bit());
        eic.intflag().modify(|_, w| w.extint15().set_bit());
    }
    COUNTER.store(COUNTER.load(Ordering::SeqCst) + 1, Ordering::SeqCst);
}
