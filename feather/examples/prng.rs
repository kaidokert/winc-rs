//! Random number Generator
//!

#![no_main]
#![no_std]

use feather as bsp;
use feather::init::init;
use feather::{debug, error, info};

use bsp::shared::SpiStream;
use feather::hal::ehal::digital::OutputPin;
use feather::shared::{create_countdowns, delay_fn};
use wincwifi::{StackError, WincClient};

fn program() -> Result<(), StackError> {
    if let Ok(mut ini) = init() {
        info!("Hello, Winc PRNG");
        let red_led = &mut ini.red_led;

        let mut cnt = create_countdowns(&ini.delay_tick);
        let mut delay_ms = delay_fn(&mut cnt.0);

        let mut stack = WincClient::new(SpiStream::new(ini.cs, ini.spi));

        let mut v = 0;
        loop {
            match stack.start_wifi_module() {
                Ok(_) => break,
                Err(nb::Error::WouldBlock) => {
                    debug!("Waiting start .. {}", v);
                    v += 1;
                    delay_ms(5)
                }
                Err(e) => return Err(e.into()),
            }
        }

        let mut random_bytes: [u8; 32] = [0; 32];
        nb::block!(stack.get_random_bytes(&mut random_bytes))?;
        #[cfg(feature = "defmt")]
        info!("Got the Random bytes: {}", random_bytes);
        #[cfg(feature = "log")]
        info!("Got the Random bytes: {:?}", random_bytes);

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
        error!("Error: {}", err);
        panic!("Error in main program");
    } else {
        info!("Good exit")
    };
    loop {}
}
