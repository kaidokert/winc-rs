#![cfg_attr(not(test), no_std)]

//! Provides a SysTick based 64-bit timer implementation.
//!
//! In addition, optionally wraps this into a basic Embassy time
//! driver.
//!
//! The timer is a standalone implementation that can be used from
//! any Cortex-M0/M3/M4/M7 code.
//!
//! Usage:
//! ```
//! use systick_timer::Timer;
//! // Set up timer with 1ms resolution, reload at 1ms, 48MHz clock
//! let timer = Timer::new(1000, 47999, 48_000_000);
//! timer.start(&mut cortex_m::Peripherals::take().unwrap().SYST);
//! // Get the current time in milliseconds
//! let now = timer.now();
//! ```
//! Alternatively, to reduce the frequency of overflow interrupts,
//! you can use the maximum reload value:
//! ```
//! let timer = Timer::new(1000, 16_777_215, 48_000_000);
//! ```
//! This generates an interrupt and reloads the timer every ~350ms, but
//! the resolution is still 1ms
//!
//! ----------------------------------------------------------------
//!
//! To use the Embassy driver, the setup needs to look as follows:
//!
//! ```ignore
//! // Create a static instance of the timer, passing in SysTick frequency
//! // and reload value.
//! embassy_time_driver::time_driver_impl!(static DRIVER: SystickDriver<4>
//!   = SystickDriver::new(8_000_000, 7999));
//! ```
//!
//! You must have a SysTick interrupt handler that calls the driver's
//!
//! ```ignore
//! #[exception]
//! fn SysTick() {
//!     DRIVER.systick_interrupt();
//! }
//! ```
//!
//! And in main, before using any timer calls, initialize the driver:
//!
//! ```ignore
//! #[embassy_executor::main]
//! async fn main(_s: embassy_executor::Spawner) {
//!   let mut periph = Peripherals::take().unwrap();
//!   DRIVER.start(&mut periph.SYST);
//!   // .. can use Timer::now() etc.
//! }
//! ```

mod timer;
pub use timer::Timer;

#[cfg(feature = "embassy-time-driver")]
mod embassy_driver;
#[cfg(feature = "embassy-time-driver")]
pub use embassy_driver::SystickDriver;
