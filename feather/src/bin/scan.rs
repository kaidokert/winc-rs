#![no_main]
#![no_std]

use feather as bsp;
use feather::init::init;

use bsp::hal::prelude::*;
use bsp::shared::{create_delay_closure, SpiStream};
use cortex_m_systick_countdown::MillisCountDown;

use wincwifi::{StackError, WincClient};

fn program() -> Result<(), StackError> {
    if let Ok((delay_tick, mut red_led, cs, spi)) = init() {
        defmt::println!("Hello, Winc scan");

        let mut countdown1 = MillisCountDown::new(&delay_tick);
        let mut countdown2 = MillisCountDown::new(&delay_tick);
        let mut countdown3 = MillisCountDown::new(&delay_tick);
        let mut delay_ms = create_delay_closure(&mut countdown1);
        let mut delay_ms2 = create_delay_closure(&mut countdown2);

        let mut stack = WincClient::new(
            SpiStream::new(cs, spi, create_delay_closure(&mut countdown3)),
            &mut delay_ms2,
        );

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
        defmt::info!("Bad error {}", err);
        panic!("Error in main program");
    } else {
        defmt::info!("Good exit")
    };
    loop {}
}
