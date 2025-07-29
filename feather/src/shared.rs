use super::hal;

use hal::prelude::*;

use core::time::Duration;

pub mod spi_stream;

pub use hal::ehal::spi::SpiBus;
pub use spi_stream::SpiStream;

pub use demos::parse_ip_octets;

use cortex_m_systick_countdown::{MillisCountDown, PollingSysTick};

fn create_delay_closure<'a>(
    delay: &'a mut MillisCountDown<'a, PollingSysTick>,
) -> impl FnMut(u32) + 'a {
    move |v: u32| {
        delay.start(Duration::from_millis(v.into()));
        nb::block!(delay.wait()).unwrap();
    }
}

// shorter alias to above
pub fn delay_fn<'a>(delay: &'a mut MillisCountDown<'a, PollingSysTick>) -> impl FnMut(u32) + 'a {
    create_delay_closure(delay)
}

// Quick helper to create 3 instances of this
// that currently every init needs
pub fn create_countdowns<'a>(
    systick: &'a PollingSysTick,
) -> (
    MillisCountDown<'a, PollingSysTick>,
    MillisCountDown<'a, PollingSysTick>,
) {
    (MillisCountDown::new(systick), MillisCountDown::new(systick))
}
