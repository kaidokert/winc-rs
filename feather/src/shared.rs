use super::hal;

use hal::sercom::spi::AnySpi;
use hal::ehal::spi::FullDuplex;
use hal::ehal::blocking::spi::Transfer;

use wincwifi::transfer::{Read /* , Write*/};

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
    fn transfer(&mut self, buf: &mut [u8]) -> Result<(), hal::sercom::spi::Error> {
        Ok(())
    }
}

impl <Spi: TransferSpi, Delay: DelayTrait> Read for Stream<Spi, Delay> {
    type ReadError = wincwifi::error::Error;

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::ReadError> {
        self.transfer(buf).map_err(|_| wincwifi::error::Error::ReadError)?;
        Ok(buf.len())
    }
}
