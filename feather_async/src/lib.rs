#![no_main]
#![no_std]

use defmt_rtt as _; // global logger

pub use bsp::hal;
pub use feather_m0 as bsp;

pub mod init;
pub mod shared;

use panic_probe as _;

#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}
