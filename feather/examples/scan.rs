//! Scan for access points
//!

#![no_main]
#![no_std]

use feather as bsp;
use feather::init2::init;

use bsp::shared::SpiStream;
use feather::hal::ehal::digital::OutputPin;
use feather::shared::{create_countdowns, delay_fn};
use wincwifi::{StackError, WincClient};

fn program() -> Result<(), StackError> {
    if let Ok(mut ini) = init() {
        defmt::println!("Hello, Winc scan");
        let red_led = &mut ini.red_led;

        let mut cnt = create_countdowns(&ini.delay_tick);
        let mut delay_ms = delay_fn(&mut cnt.0);
        let mut delay_ms2 = delay_fn(&mut cnt.1);

        let mut stack = WincClient::new(SpiStream::new(ini.cs, ini.spi), &mut delay_ms2);

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

        delay_ms(1000);
        defmt::info!("Scanning for access points ..");
        let num_aps = nb::block!(stack.scan())?;
        defmt::info!("Scan done, aps:{}", num_aps);

        for i in 0..num_aps {
            let result = nb::block!(stack.get_scan_result(i))?;
            defmt::info!(
                "Scan strings: [{}] '{}' rssi:{} ch:{} {} {=[u8]:#x}",
                i,
                result.ssid.as_str(),
                result.rssi,
                result.channel,
                result.auth,
                result.bssid
            );
        }

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
