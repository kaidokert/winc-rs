use super::hal;
use defmt::trace;

use hal::gpio::AnyPin;

use hal::ehal::digital::OutputPin;
use wincwifi::transfer::Xfer;

use super::SpiBus;
use core::mem::take;

// Helper trait to define the signature once
pub trait DelayFunc: FnMut(u32) {}
impl<U> DelayFunc for U where U: FnMut(u32) {}

// TODO: Maybe this doesn't need a delay at all
pub struct SpiStream<CS: AnyPin, Spi: SpiBus, Delay: DelayFunc> {
    cs: Option<CS>,
    spi: Spi,
    // Alternative: delay: &'a mut dyn FnMut(u32) as a borrow
    delay: Delay,
}

impl<CS: AnyPin, Spi: SpiBus, Delay: DelayFunc> SpiStream<CS, Spi, Delay> {
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

impl<CS: AnyPin, Spi: SpiBus, Delay: DelayFunc> Xfer for SpiStream<CS, Spi, Delay> {
    fn recv(&mut self, dest: &mut [u8]) -> Result<(), wincwifi::errors::Error> {
        self.transfer(dest)
            .map_err(|_| wincwifi::errors::Error::ReadError)?;
        trace!("Stream: read {} {=[u8]:#x} bytes", dest.len(), dest);
        Ok(())
    }

    fn send(&mut self, src: &[u8]) -> Result<(), wincwifi::errors::Error> {
        let mut tmp = [0; 256];
        let tmp_slice = &mut tmp[0..src.len()];
        tmp_slice.clone_from_slice(src);
        trace!("Stream: writing {=[u8]:#x} bytes", src);
        self.transfer(tmp_slice)
            .map_err(|_| wincwifi::errors::Error::WriteError)?;
        Ok(())
    }
}
