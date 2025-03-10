//! Demonstrates a telnet shell
//!
//! Starts a stub command shell on default telnet port 23
#![no_main]
#![no_std]

use feather as bsp;
use feather::init::init;

use bsp::shared::SpiStream;
use demos::telnet_shell;
use feather::shared::{create_countdowns, delay_fn};
use wincwifi::{StackError, WincClient};

fn program() -> Result<(), StackError> {
    if let Ok(ini) = init() {
        defmt::println!("Hello, telnet shell!");
        let mut cnt = create_countdowns(&ini.delay_tick);

        let mut delay_ms = delay_fn(&mut cnt.0);

        defmt::info!("Connecting to saved network ..",);
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
        defmt::info!("Started, connecting to AP ..");
        nb::block!(stack.connect_to_saved_ap())?;
        delay_ms(1000);

        let info = nb::block!(stack.get_connection_info())?;
        defmt::info!("Connection info: {}", info);

        defmt::info!(".. connected to AP, running telnet shell ..");

        telnet_shell::telnet_shell(&mut stack, None)?;
    }
    Ok(())
}

#[cortex_m_rt::entry]
fn main() -> ! {
    if let Err(err) = program() {
        defmt::error!("Error: {:?}", err);
        panic!("Error in main program");
    } else {
        defmt::info!("Good exit")
    };
    loop {}
}
