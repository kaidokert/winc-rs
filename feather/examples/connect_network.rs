//! Connect to an access point
//! Credentials are passed as env vars at build time

#![no_main]
#![no_std]

use bsp::shared::SpiStream;
use feather as bsp;
use feather::hal::ehal::digital::OutputPin;
use feather::init::init;
use feather::shared::{create_countdowns, delay_fn};

use feather::bsp::pac;
use pac::interrupt;

const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";

use wincwifi::{StackError, WincClient};

fn program() -> Result<(), StackError> {
    if let Ok(mut ini) = init() {
        defmt::println!("Hello, Winc Module");
        let delay_tick = &mut ini.delay_tick;
        let red_led = &mut ini.red_led;

        let mut cnt = create_countdowns(&delay_tick);

        let mut delay_ms = delay_fn(&mut cnt.0);

        let ssid = option_env!("TEST_SSID").unwrap_or(DEFAULT_TEST_SSID);
        let password = option_env!("TEST_PASSWORD").unwrap_or(DEFAULT_TEST_PASSWORD);
        defmt::info!(
            "Connecting to network: {} with password: {}",
            ssid,
            password
        );
        let mut stack = WincClient::new(SpiStream::new(ini.cs, ini.spi));

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
            red_led.set_high().unwrap();
            delay_ms(200);
            red_led.set_low().unwrap();
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

#[interrupt]
fn EIC() {
    unsafe {
        // Accessing registers from interrupts context is safe
        let eic = &*pac::Eic::ptr();

        let flag5 = eic.intflag().read().extint5().bit_is_set();
        if flag5 {
            eic.intflag().modify(|_, w| w.extint5().set_bit());
        }

        let flag15 = eic.intflag().read().extint15().bit_is_set();
        if flag15 {
            eic.intflag().modify(|_, w| w.extint15().set_bit());
        }
    }
}
