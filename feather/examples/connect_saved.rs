//! Connect to a saved access point
//! Credentials are saved on the module on a previous
//! successful connection

#![no_main]
#![no_std]

use bsp::shared::SpiStream;
use feather as bsp;
use feather::hal::ehal::digital::OutputPin;
use feather::init::init;
use feather::shared::{create_countdowns, delay_fn};
use feather::{debug, error, info};

use wincwifi::{StackError, WincClient};

fn program() -> Result<(), StackError> {
    if let Ok(mut ini) = init() {
        info!("Hello, Winc Module");
        let red_led = &mut ini.red_led;

        let mut cnt = create_countdowns(&ini.delay_tick);

        let mut delay_ms = delay_fn(&mut cnt.0);

        info!("Connecting to saved network ..",);
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

        info!("Started, connecting to AP ..");
        nb::block!(stack.connect_to_saved_ap())?;

        delay_ms(1000);
        let info = nb::block!(stack.get_connection_info())?;
        info!("Connection info: {}", info);

        info!(".. connected to AP, going to loop");
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
