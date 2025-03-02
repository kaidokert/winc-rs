#![no_main]
#![no_std]
#![allow(unused_imports)]

use bsp::hal::prelude::*;
use bsp::shared::{parse_ip_octets, SpiStream};
use feather as bsp;
use feather::init::init;
use feather::shared::{create_countdowns, delay_fn};

use wincwifi::{StackError, WincClient};

use core::convert::Infallible;
use core::net::Ipv4Addr;

use core::str::FromStr;

use demos::iperf3_client::iperf3_client;

const DEFAULT_TEST_IP: &str = "34.19.56.238";
const DEFAULT_TEST_PORT: &str = "5201";

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
    if let Ok((delay_tick, mut red_led, cs, spi)) = init() {
        defmt::println!("Hello, Iperf ");

        let mut cnt = create_countdowns(&delay_tick);

        let mut delay_ms = delay_fn(&mut cnt.0);
        let mut delay_ms2 = delay_fn(&mut cnt.1);

        defmt::info!("Connecting to saved network ..",);
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
        defmt::info!("Started, connecting to AP ..");
        nb::block!(stack.connect_to_saved_ap())?;
        delay_ms(1000);

        let info = nb::block!(stack.get_connection_info())?;
        defmt::info!("Connection info: {}", info);

        defmt::info!(".. connected to AP, running iperf3 ..");

        let test_ip = option_env!("TEST_IP").unwrap_or(DEFAULT_TEST_IP);
        let ip_values: [u8; 4] = parse_ip_octets(test_ip);
        let server_addr = Ipv4Addr::new(ip_values[0], ip_values[1], ip_values[2], ip_values[3]);
        let test_port = option_env!("TEST_PORT").unwrap_or(DEFAULT_TEST_PORT);
        let port = u16::from_str(test_port).unwrap_or(12345);

        // read systick value from SYST, it's somewhat random by now after
        // waiting for the AP to connect
        let systick = SYST::get_current();
        let mut fake_rng = FakeRng { init: systick };
        iperf3_client(&mut stack, server_addr, Some(port), &mut fake_rng)?;

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

// Todo: embedded-nal iperf3
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
