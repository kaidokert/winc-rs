//! A mandatory blinky to verify the board is working

#![no_main]
#![no_std]

use bsp::hal::ehal::digital::OutputPin;
use bsp::hal::prelude::*;
use core::sync::atomic::{AtomicU32, Ordering};
use cortex_m::peripheral::SYST;
use feather as bsp;
use feather::init::init;

// Global counter for SYSTICK overflows
static OVERFLOW_COUNT: AtomicU32 = AtomicU32::new(0);

#[cortex_m_rt::exception]
fn SysTick() {
    // Increment the overflow counter
    OVERFLOW_COUNT.store(
        OVERFLOW_COUNT.load(Ordering::Relaxed) + 1,
        Ordering::Relaxed,
    );
}

#[cortex_m_rt::entry]
fn main() -> ! {
    if let Ok(mut ini) = init() {
        let delay = &mut ini.delay_tick;
        let red_led = &mut ini.red_led;

        // Enable SYSTICK interrupt
        let systick = unsafe { &*SYST::ptr() };
        unsafe {
            // Enable SYSTICK interrupt
            systick.csr.modify(|r| r | 1 << 1); // Set TICKINT bit
        }

        defmt::println!("Hello, blinky timer!");
        loop {
            delay.delay_ms(200u32);
            red_led.set_high().unwrap();
            delay.delay_ms(200u32);
            red_led.set_low().unwrap();

            // Calculate seconds from overflow count (each overflow is 10ms)
            let overflows = OVERFLOW_COUNT.load(Ordering::Relaxed);
            let seconds = overflows / 1000; // 1000 overflows = 1 second (10ms * 1000 = 10s)

            defmt::println!("Elapsed time: {=u32} seconds", seconds);
        }
    } else {
        panic!("Failed to initialize");
    }
}
