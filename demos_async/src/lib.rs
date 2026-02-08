#![no_std]
// Dual logging system compatibility: defmt doesn't support modern format syntax
#![allow(clippy::uninlined_format_args)]

// Compile-time checks for logging features
#[cfg(all(feature = "defmt", feature = "log"))]
compile_error!("Features 'defmt' and 'log' are mutually exclusive. Enable only one for logging.");

#[cfg(not(any(feature = "defmt", feature = "log")))]
compile_error!("Must enable either 'defmt' or 'log' feature for logging support.");

pub mod udp_client;
pub mod udp_server;

// Re-export logging macros for convenience
#[cfg(feature = "defmt")]
pub use defmt::{debug, error, info, trace};

#[cfg(feature = "log")]
pub use log::{debug, error, info, trace};
