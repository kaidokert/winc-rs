//! Scan for access points
//!

#![no_main]
#![no_std]

use cortex_m_systick_countdown::{MillisCountDown, PollingSysTick};
use feather as bsp;
use feather::init::init;

use bsp::shared::SpiStream;
use feather::hal::ehal::digital::OutputPin;
use feather::shared::{create_countdowns, delay_fn};
use wincwifi::{StackError, WincClient};

use core::sync::atomic::{AtomicUsize, Ordering};

use feather::bsp::pac;
use pac::interrupt;

use core::time::Duration;
use feather::hal::prelude::*;

static CNT5: AtomicUsize = AtomicUsize::new(0);
static CNT15: AtomicUsize = AtomicUsize::new(0);

static TIMES_CALLED: AtomicUsize = AtomicUsize::new(0);
static TOTAL_TIME: AtomicUsize = AtomicUsize::new(0);

fn program() -> Result<(), StackError> {
    if let Ok(mut ini) = init() {
        defmt::println!("Hello, Winc scan");
        let red_led = &mut ini.red_led;

        let mut prev_5_value: usize = CNT5.load(Ordering::SeqCst);
        let mut prev_15_value: usize = CNT15.load(Ordering::SeqCst);

        let mut cnt = create_countdowns(&ini.delay_tick);
        let mut delay_ms = delay_fn(&mut cnt.0);

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
        let scan_loop_counter = AtomicUsize::new(0);

        let scan = |stack: &mut WincClient<_>| -> Result<(), StackError> {
            defmt::info!("Scanning for access points ..");
            let num_aps = loop {
                match stack.scan() {
                    Ok(num_aps) => break num_aps,
                    Err(nb::Error::WouldBlock) => {
                        let loop_count = scan_loop_counter.load(Ordering::SeqCst);
                        scan_loop_counter.store(loop_count + 1, Ordering::SeqCst);
                    }
                    Err(e) => return Err(e.into()),
                }
            };
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
            Ok(())
        };
        delay_ms(1000);
        scan(&mut stack)?;

        loop {
            delay_ms(200);
            red_led.set_high().unwrap();
            delay_ms(200);
            red_led.set_low().unwrap();
            stack.heartbeat().unwrap();
            let new_value = CNT15.load(Ordering::SeqCst);
            if new_value != prev_15_value {
                prev_15_value = new_value;
                defmt::println!("Button counter: {}", new_value);
                // button press, trigger scan
                scan(&mut stack)?;
            }
            let new_value = CNT5.load(Ordering::SeqCst);
            if new_value != prev_5_value {
                prev_5_value = new_value;
                defmt::println!("Wifi IRQ counter: {}", new_value);
                defmt::println!("Times called: {}", TIMES_CALLED.load(Ordering::SeqCst));
                defmt::println!("Total time: {}", TOTAL_TIME.load(Ordering::SeqCst));
                defmt::println!("Debug info: {:?}", stack.get_debug_info());
                defmt::println!(
                    "Scan loop counter: {}",
                    scan_loop_counter.load(Ordering::SeqCst)
                );
            }
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
            CNT5.store(CNT5.load(Ordering::SeqCst) + 1, Ordering::SeqCst);
            eic.intflag().modify(|_, w| w.extint5().set_bit());
        }

        let flag15 = eic.intflag().read().extint15().bit_is_set();
        if flag15 {
            CNT15.store(CNT15.load(Ordering::SeqCst) + 1, Ordering::SeqCst);
            eic.intflag().modify(|_, w| w.extint15().set_bit());
        }
    }
}
