use super::hal;

use hal::sercom::spi::AnySpi;
use hal::ehal::spi::FullDuplex;
use hal::ehal::blocking::spi::Transfer;

pub trait DelayTrait: FnMut(u32) {}
impl<U> DelayTrait for U where U: FnMut(u32) {}

pub trait TransferSpi: AnySpi + Transfer<u8, Error = hal::sercom::spi::Error> {}
impl<U> TransferSpi for U
where
    U: AnySpi,
    U: Transfer<u8, Error = hal::sercom::spi::Error>,
    U: FullDuplex<u8>,
{
}


pub struct Stream<Spi: TransferSpi, Delay: DelayTrait>{
    spi: Spi,
    delay: Delay
}

impl<Spi: TransferSpi, Delay: DelayTrait> Stream<Spi, Delay> {
    pub fn new(spi: Spi, delay: Delay) -> Self {
        Stream {
            spi,
            delay
        }
    }
}