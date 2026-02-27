use super::hal;

pub mod error;
pub mod spi_stream;

pub use error::{AppError, FailureSource};
pub use hal::ehal::spi::SpiBus;
pub use spi_stream::SpiStream;
