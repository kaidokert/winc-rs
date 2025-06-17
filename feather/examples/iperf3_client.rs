//! Demonstrates the iperf3 client
//!
//! Run this against a `iperf3 -s` server somewhere
//!
#![no_main]
#![no_std]
#![allow(unused_imports)]

use bsp::hal::ehal::digital::OutputPin;
use bsp::hal::prelude::*;
use bsp::shared::{parse_ip_octets, SpiStream};
use feather as bsp;
use feather::init::init;
use feather::shared::{create_countdowns, delay_fn};

use wincwifi::{StackError, WincClient};

use core::convert::Infallible;
use core::net::Ipv4Addr;

use core::str::FromStr;

use demos::iperf3_client::{iperf3_client, Conf, TestConfig};

const DEFAULT_IPERF_IP: &str = "192.168.1.1";
const DEFAULT_IPERF_PORT: &str = "5201";

const MAX_BLOCK_LEN: usize = 8192;

use cortex_m::peripheral::{syst::SystClkSource, SYST};

struct FakeRng {
    init: u32,
}

impl demos::iperf3_client::RngCore for FakeRng {
    fn next_u32(&mut self) -> u32 {
        self.init ^= self.init << 13;
        self.init ^= self.init >> 17;
        self.init ^= self.init << 5;
        self.init
    }
    fn next_u64(&mut self) -> u64 {
        todo!()
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for byte in dest.iter_mut() {
            *byte = self.next_u32() as u8;
        }
    }
}

#[derive(Debug)]
enum Err<Inner> {
    Stack(StackError),
    Iperf(demos::iperf3_client::Errors),
    Nb(Inner),
    NbWouldBlock,
}
impl<T> From<StackError> for Err<T> {
    fn from(err: StackError) -> Self {
        Err::Stack(err)
    }
}
impl<T> From<demos::iperf3_client::Errors> for Err<T> {
    fn from(err: demos::iperf3_client::Errors) -> Self {
        Err::Iperf(err)
    }
}
impl<Inner> From<nb::Error<Inner>> for Err<Inner> {
    fn from(err: nb::Error<Inner>) -> Self {
        match err {
            nb::Error::WouldBlock => Err::NbWouldBlock,
            nb::Error::Other(e) => Err::Nb(e),
        }
    }
}
impl<T> From<Infallible> for Err<T> {
    fn from(_: Infallible) -> Self {
        unreachable!()
    }
}
impl<T> defmt::Format for Err<T>
where
    T: defmt::Format,
{
    fn format(&self, f: defmt::Formatter) {
        match self {
            Err::Stack(err) => defmt::write!(f, "Stack({})", err),
            Err::Iperf(err) => defmt::write!(f, "Iperf({})", err),
            Err::Nb(err) => defmt::write!(f, "Nb({})", err),
            Err::NbWouldBlock => defmt::write!(f, "NbWouldBlock"),
        }
    }
}

fn program<T>() -> Result<(), Err<T>>
where
    Err<T>: From<nb::Error<StackError>>,
{
    if let Ok(mut ini) = init() {
        defmt::println!("Hello, Iperf ");
        let red_led = &mut ini.red_led;

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

        defmt::info!(".. connected to AP, running iperf3 ..");

        let test_ip = option_env!("TEST_IPERF_IP").unwrap_or(DEFAULT_IPERF_IP);
        let ip_values: [u8; 4] = parse_ip_octets(test_ip);
        let server_addr = Ipv4Addr::new(ip_values[0], ip_values[1], ip_values[2], ip_values[3]);
        let test_port = option_env!("TEST_IPERF_PORT").unwrap_or(DEFAULT_IPERF_PORT);
        let port = u16::from_str(test_port).unwrap_or(12345);

        let use_udp = option_env!("TEST_IPERF_UDP").unwrap_or("false");
        let use_udp = bool::from_str(use_udp).unwrap_or(false);

        let numbytes = match option_env!("NUM_BYTES") {
            Some(numbytes) => numbytes.parse::<usize>().unwrap(),
            None => 256,
        };
        let block_len = match option_env!("BLOCK_LEN") {
            Some(block) => block.parse::<usize>().unwrap(),
            None => 32,
        };

        let conf = TestConfig {
            conf: Conf::Bytes(numbytes),
            transmit_block_len: block_len,
        };

        let systick = SYST::get_current();
        let mut fake_rng = FakeRng { init: systick };
        iperf3_client::<MAX_BLOCK_LEN, _, _, _>(
            &mut stack,
            server_addr,
            Some(port),
            &mut fake_rng,
            Some(conf),
            use_udp,
            &mut delay_ms,
        )?;

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
