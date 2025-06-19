use core::str::FromStr;

use super::hal;

pub mod spi_stream;

pub use hal::ehal::spi::SpiBus;
pub use spi_stream::SpiStream;

// TODO: Remove this fn and just use Ipv4Addr::from_str directly
pub fn parse_ip_octets(ip: &str) -> [u8; 4] {
    let mut octets = [0; 4];
    let addr = core::net::Ipv4Addr::from_str(ip).unwrap();
    octets[0] = addr.octets()[0];
    octets[1] = addr.octets()[1];
    octets[2] = addr.octets()[2];
    octets[3] = addr.octets()[3];
    octets
}
