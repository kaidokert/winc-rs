use super::hal;

use hal::ehal::timer::CountDown;
use hal::prelude::*;

use core::time::Duration;

pub mod delay_trait;
pub mod spi_stream;
pub mod transfer_spi;

use delay_trait::DelayTrait;
pub use spi_stream::SpiStream;
pub use transfer_spi::TransferSpi;

pub fn create_delay_closure<'a, C>(delay: &'a mut C) -> impl FnMut(u32) + 'a
where
    C: CountDown<Time = Duration> + 'a,
{
    move |v: u32| {
        delay.start(Duration::from_millis(v.into()));
        nb::block!(delay.wait()).unwrap();
    }
}
