use super::hal;
use hal::ehal::blocking::spi::Transfer;
use hal::ehal::spi::FullDuplex;
use hal::sercom::spi::AnySpi;

pub trait TransferSpi: AnySpi + Transfer<u8, Error = hal::sercom::spi::Error> {}
impl<U> TransferSpi for U
where
    U: AnySpi,
    U: Transfer<u8, Error = hal::sercom::spi::Error>,
    U: FullDuplex<u8>,
{
}
