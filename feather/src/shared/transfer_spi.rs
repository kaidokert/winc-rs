use super::hal;
use embedded_hal_02::blocking::spi::Transfer;
use embedded_hal_02::spi::FullDuplex;
use hal::sercom::spi::AnySpi;

pub trait TransferSpi: AnySpi + Transfer<u8, Error = hal::sercom::spi::Error> {}
impl<U> TransferSpi for U
where
    U: AnySpi,
    U: Transfer<u8, Error = hal::sercom::spi::Error>,
    U: FullDuplex<u8>,
{
}
