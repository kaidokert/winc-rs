use super::hal;

pub mod delay_trait;
pub mod transfer_spi;
pub mod spi_stream;

use delay_trait::DelayTrait;
pub use transfer_spi::TransferSpi;
pub use spi_stream::Stream;
