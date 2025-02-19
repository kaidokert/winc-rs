use super::hal;

use hal::prelude::*;

use core::time::Duration;

pub mod spi_stream;

pub use hal::ehal::spi::SpiBus;
pub use spi_stream::SpiStream;

use cortex_m_systick_countdown::{MillisCountDown, PollingSysTick};

pub fn create_delay_closure<'a>(
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

pub fn parse_ip_octets(ip: &str) -> [u8; 4] {
    let mut octets = [0; 4];
    let mut octet_index = 0;
    let mut current_value = 0;

    ip.bytes().for_each(|byte| match byte {
        b'0'..=b'9' => current_value = current_value * 10 + (byte - b'0'),
        b'.' => {
            octets[octet_index] = current_value;
            octet_index += 1;
            current_value = 0;
        }
        _ => {}
    });

    octets[octet_index] = current_value;
    octets
}

// Quick helper to create 3 instances of this
// that currently every init needs
pub fn create_3_countdowns<'a>(
    systick: &'a PollingSysTick,
) -> (
    MillisCountDown<'a, PollingSysTick>,
    MillisCountDown<'a, PollingSysTick>,
    MillisCountDown<'a, PollingSysTick>,
) {
    (
        MillisCountDown::new(systick),
        MillisCountDown::new(systick),
        MillisCountDown::new(systick),
    )
}
