use super::hal;
use defmt::trace;

use hal::gpio::AnyPin;

use hal::ehal::digital::OutputPin;
use wincwifi::Transfer;

use super::SpiBus;
use core::mem::take;

const DEFAULT_WAIT_CYCLES: u32 = 16_000; // hand tested :)
const FAST_WAIT_CYCLES: u32 = 500; // ditto

// This shouldn't use AnyPin, but ehal::digital::OutputPin
pub struct SpiStream<CS: AnyPin, Spi: SpiBus> {
    cs: Option<CS>,
    spi: Spi,
    wait_cycles: u32,
}

const TRANSFER_SIZE: usize = 256;

impl<CS: AnyPin, Spi: SpiBus> SpiStream<CS, Spi> {
    pub fn new(cs: CS, spi: Spi) -> Self {
        SpiStream {
            cs: Some(cs),
            spi,
            wait_cycles: DEFAULT_WAIT_CYCLES,
        }
    }
    fn set_wait_cycles(&mut self, wait_cycles: u32) {
        self.wait_cycles = wait_cycles;
    }
    fn transfer(&mut self, buf: &mut [u8], start: bool, end: bool) -> Result<(), Spi::Error> {
        if let Some(cs) = take(&mut self.cs) {
            trace!("send: {=[u8]:#x}", buf);
            let mut pin = cs.into().into_push_pull_output();

            if start {
                pin.set_low().ok();
                cortex_m::asm::delay(self.wait_cycles);
            }
            self.spi.transfer_in_place(buf)?;
            cortex_m::asm::delay(self.wait_cycles);
            if end {
                pin.set_high().ok();
            }

            self.cs.get_or_insert(pin.into_mode().into());
            trace!("recv: {=[u8]:#x}", buf);
        }
        Ok(())
    }
}

impl<CS: AnyPin, Spi: SpiBus> Transfer for SpiStream<CS, Spi> {
    fn recv(&mut self, dest: &mut [u8]) -> Result<(), wincwifi::CommError> {
        self.transfer(dest, true, true)
            .map_err(|_| wincwifi::CommError::ReadError)?;
        trace!("Stream: read {} {=[u8]:#x} bytes", dest.len(), dest);
        Ok(())
    }

    // todo: Maybe self.spi.write() can be made to work here ?
    fn send(&mut self, src: &[u8]) -> Result<(), wincwifi::CommError> {
        // Need a mutable buffer for send, so copy to tmp
        let mut tmp = [0; TRANSFER_SIZE];
        for (i, chunk) in src.chunks(TRANSFER_SIZE).enumerate() {
            let tmp_slice = &mut tmp[..chunk.len()];
            tmp_slice.clone_from_slice(chunk);

            let is_first = i == 0;
            let is_last = chunk.len() < TRANSFER_SIZE || i == src.len() / TRANSFER_SIZE;

            trace!("Stream: writing {=[u8]:#x} bytes", chunk);
            self.transfer(tmp_slice, is_first, is_last)
                .map_err(|_| wincwifi::CommError::WriteError)?;
        }

        Ok(())
    }
    fn switch_to_high_speed(&mut self) {
        self.set_wait_cycles(FAST_WAIT_CYCLES);
    }
    fn delay(&mut self, delay: u32) {
        cortex_m::asm::delay(delay * 100_000);
    }
    fn delay_us(&mut self, delay: u32) {
        cortex_m::asm::delay(delay * 100);
    }
}
