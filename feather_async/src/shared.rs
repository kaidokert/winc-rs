use super::hal;

pub mod spi_stream;

pub use hal::ehal::spi::SpiBus;
pub use spi_stream::SpiStream;
