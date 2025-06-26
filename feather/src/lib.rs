#![no_main]
#![no_std]

// Compile-time checks for logging features
#[cfg(all(feature = "defmt", feature = "log"))]
compile_error!("Features 'defmt' and 'log' are mutually exclusive. Enable only one for logging.");

#[cfg(not(any(feature = "defmt", feature = "log")))]
compile_error!("Must enable either 'defmt' or 'log' feature for logging support.");

#[cfg(feature = "defmt")]
pub use defmt::{debug, error, info, trace, warn};

#[cfg(feature = "log")]
pub use log::{debug, error, info, trace, warn};

#[cfg(feature = "defmt")]
use defmt_rtt as _; // global logger

pub use bsp::hal;
pub use feather_m0 as bsp;

pub mod init;
pub mod shared;

#[cfg(feature = "defmt")]
use panic_probe as _;

// same panicking *behavior* as `panic-probe` but doesn't print a panic message
// this prevents the panic message being printed *twice* when `defmt::panic` is invoked
#[cfg(feature = "defmt")]
#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}
