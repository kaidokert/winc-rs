#![no_main]
#![no_std]

use core::net::Ipv4Addr;

use bsp::hal::prelude::*;
use bsp::shared::SpiStream;
use feather as bsp;
use feather::init::init;
use feather::shared::{create_3_countdowns, delay_fn, parse_ip_octets};

use core::str::FromStr;

use wincwifi::{StackError, WincClient};

const DEFAULT_TEST_IP: &str = "192.168.1.1";
const DEFAULT_TEST_TTL: u8 = 200;
const DEFAULT_TEST_COUNT: u16 = 4;

fn program() -> Result<(), StackError> {
    if let Ok((delay_tick, mut red_led, cs, spi)) = init() {
        defmt::println!("Hello, Winc ping");

        let mut cnt = create_3_countdowns(&delay_tick);
        let mut delay1 = delay_fn(&mut cnt.1);
        let mut delay_ms = delay_fn(&mut cnt.2);

        defmt::info!("Connecting to saved network ..",);
        let mut stack = WincClient::new(SpiStream::new(cs, spi, delay_fn(&mut cnt.0)), &mut delay1);

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

        for _ in 0..20 {
            stack.heartbeat().unwrap();
            delay_ms(200);
        }

        let test_ip = option_env!("TEST_IP").unwrap_or(DEFAULT_TEST_IP);
        let test_ttl = option_env!("TEST_TTL").unwrap_or("");
        let test_ttl = u8::from_str(test_ttl).unwrap_or(DEFAULT_TEST_TTL);
        let test_count = option_env!("TEST_COUNT").unwrap_or("");
        let test_count = u16::from_str(test_count).unwrap_or(DEFAULT_TEST_COUNT);
        defmt::info!("Connected sending ping to {}", test_ip);
        let ip_values: [u8; 4] = parse_ip_octets(test_ip);
        let ip = Ipv4Addr::new(ip_values[0], ip_values[1], ip_values[2], ip_values[3]);
        let ping_result = nb::block!(stack.send_ping(ip, test_ttl, test_count))?;

        let success_pct = ping_result.num_successful as f32
            / (ping_result.num_successful + ping_result.num_failed) as f32
            * 100.0;
        defmt::info!("ping result: {:?} success: {:?}%", ping_result, success_pct);

        defmt::info!(".. ping completed, going to loop");
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
