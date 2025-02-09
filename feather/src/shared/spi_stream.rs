use super::hal;
use defmt::trace;

use hal::gpio::AnyPin;

use hal::ehal::digital::OutputPin;
use wincwifi::transfer::{Read, Write};

use super::DelayTrait;
use super::TransferSpi;
use core::mem::take;

// TODO: Maybe this doesn't need a delay
pub struct SpiStream<CS: AnyPin, Spi: TransferSpi, Delay: DelayTrait> {
    cs: Option<CS>,
    spi: Spi,
    delay: Delay,
}

impl<CS: AnyPin, Spi: TransferSpi, Delay: DelayTrait> SpiStream<CS, Spi, Delay> {
    pub fn new(cs: CS, spi: Spi, delay: Delay) -> Self {
        SpiStream {
            cs: Some(cs),
            spi,
            delay,
        }
    }
    fn transfer(&mut self, buf: &mut [u8]) -> Result<(), Spi::Error> {
        const WAIT_MS: u32 = 1;
        if let Some(cs) = take(&mut self.cs) {
            let mut pin = cs.into().into_push_pull_output();
            pin.set_low().ok();

            trace!("send: {=[u8]:#x}", buf);
            (self.delay)(WAIT_MS);
            self.spi.transfer_in_place(buf)?;
            (self.delay)(WAIT_MS);
            trace!("recv: {=[u8]:#x}", buf);

            pin.set_high().ok();

            self.cs.get_or_insert(pin.into_mode().into());
        }
        Ok(())
    }
}

impl<CS: AnyPin, Spi: TransferSpi, Delay: DelayTrait> Read for SpiStream<CS, Spi, Delay> {
    type ReadError = wincwifi::errors::Error;

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::ReadError> {
        self.transfer(buf)
            .map_err(|_| wincwifi::errors::Error::ReadError)?;
        trace!("Stream: read {} {=[u8]:#x} bytes", buf.len(), buf);
        Ok(buf.len())
    }
}

impl<CS: AnyPin, Spi: TransferSpi, Delay: DelayTrait> Write for SpiStream<CS, Spi, Delay> {
    type WriteError = wincwifi::errors::Error;

    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::WriteError> {
        // TODO : Maybe we can do away with the copy and fixed buffer and not panic here
        // or at least loop
        let mut tmp = [0; 256];
        let tmp_slice = &mut tmp[0..buf.len()];
        tmp_slice.clone_from_slice(buf);
        trace!("Stream: writing {=[u8]:#x} bytes", buf);
        self.transfer(tmp_slice)
            .map_err(|_| wincwifi::errors::Error::WriteError)?;
        Ok(buf.len())
    }
}
