use super::hal;

use hal::prelude::*;

use core::{str::FromStr, time::Duration};

pub mod spi_stream;

pub use hal::ehal::spi::SpiBus;
pub use spi_stream::SpiStream;

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

// TODO: Remove this fn and just use Ipv4Addr::from_str directly
pub fn parse_ip_octets(ip: &str) -> Result<[u8; 4], core::net::AddrParseError> {
    core::net::Ipv4Addr::from_str(ip).map(|addr| addr.octets())
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
