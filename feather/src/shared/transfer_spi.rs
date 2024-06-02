
use super::hal;
use hal::sercom::spi::AnySpi;
use hal::ehal::blocking::spi::Transfer;
use hal::ehal::spi::FullDuplex;

pub trait TransferSpi: AnySpi + Transfer<u8, Error = hal::sercom::spi::Error> {}
impl<U> TransferSpi for U
where
    U: AnySpi,
    U: Transfer<u8, Error = hal::sercom::spi::Error>,
    U: FullDuplex<u8>,
{
}
