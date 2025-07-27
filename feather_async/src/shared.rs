use core::str::FromStr;

use super::hal;

pub mod spi_stream;

pub use hal::ehal::spi::SpiBus;
pub use spi_stream::SpiStream;

// TODO: Remove this fn and just use Ipv4Addr::from_str directly
pub fn parse_ip_octets(ip: &str) -> Result<[u8; 4], core::net::AddrParseError> {
    core::net::Ipv4Addr::from_str(ip).map(|addr| addr.octets())
}
