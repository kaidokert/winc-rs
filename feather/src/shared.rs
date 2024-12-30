use super::hal;


use hal::prelude::*;

use core::time::Duration;

pub mod delay_trait;
pub mod spi_stream;
pub mod transfer_spi;

use delay_trait::DelayTrait;
pub use spi_stream::SpiStream;
pub use transfer_spi::TransferSpi;

use cortex_m_systick_countdown::{PollingSysTick,MillisCountDown};

pub fn create_delay_closure<'a>(
    delay: &'a mut MillisCountDown<'a, PollingSysTick>,
) -> impl FnMut(u32) + 'a {
    move |v: u32| {
        delay.start(Duration::from_millis(v.into()));
        nb::block!(delay.wait()).unwrap();
    }
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
