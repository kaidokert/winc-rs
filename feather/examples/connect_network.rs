//! Connect to an access point
//! Credentials are passed as env vars at build time

#![no_main]
#![no_std]

use bsp::hal::prelude::*;
use bsp::shared::SpiStream;
use feather as bsp;
use feather::init::init;
use feather::shared::{create_countdowns, delay_fn};

const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";

use wincwifi::{StackError, WincClient};

fn program() -> Result<(), StackError> {
    if let Ok((delay_tick, mut red_led, cs, spi)) = init() {
        defmt::println!("Hello, Winc Module");

        let mut cnt = create_countdowns(&delay_tick);

        let mut delay_ms = delay_fn(&mut cnt.0);
        let mut delay_ms2 = delay_fn(&mut cnt.1);

        let ssid = option_env!("TEST_SSID").unwrap_or(DEFAULT_TEST_SSID);
        let password = option_env!("TEST_PASSWORD").unwrap_or(DEFAULT_TEST_PASSWORD);
        defmt::info!(
            "Connecting to network: {} with password: {}",
            ssid,
            password
        );
        let mut stack = WincClient::new(SpiStream::new(cs, spi), &mut delay_ms2);

        let mut v = 0;
        loop {
            match stack.start_wifi_module() {
                Ok(_) => break,
                Err(nb::Error::WouldBlock) => {
                    defmt::debug!("Waiting start .. {}", v);
                    v += 1;
                    delay_ms(5)
                }
                Err(e) => return Err(e.into()),
            }
        }

        for _ in 0..20 {
            stack.heartbeat().unwrap();
            delay_ms(200);
        }

        defmt::info!("Started, connecting to AP ..");
        nb::block!(stack.connect_to_ap(ssid, password, false))?;

        defmt::info!(".. connected to AP, going to loop");
        loop {
            delay_ms(200);
            red_led.set_high()?;
            delay_ms(200);
            red_led.set_low()?;
            stack.heartbeat().unwrap();
        }
    }
    Ok(())
}

#[cortex_m_rt::entry]
fn main() -> ! {
    if let Err(err) = program() {
        defmt::error!("Error: {}", err);
        panic!("Error in main program");
    } else {
        defmt::info!("Good exit")
    };
    loop {}
}
