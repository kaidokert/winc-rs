use cortex_m_systick_countdown::MillisCountDown;
use embedded_nal::{Dns, TcpClientStack, TcpFullStack, UdpClientStack, UdpFullStack};
use feather::{
    init::init,
    shared::{create_delay_closure, SpiStream},
};

use super::bsp::hal::prelude::*;
use wincwifi::Handle;

use wincwifi::WincClient;

use crate::{DEFAULT_TEST_PASSWORD, DEFAULT_TEST_SSID};

pub type MyTcpClientStack<'a> =
    &'a mut dyn TcpClientStack<TcpSocket = Handle, Error = wincwifi::StackError>;

pub type MyUdpClientStack<'a> =
    &'a mut dyn UdpClientStack<UdpSocket = Handle, Error = wincwifi::StackError>;

pub type MyUdpFullStack<'a> =
    &'a mut dyn UdpFullStack<UdpSocket = Handle, Error = wincwifi::StackError>;

pub type MyTcpFullStack<'a> =
    &'a mut dyn TcpFullStack<TcpSocket = Handle, Error = wincwifi::StackError>;

pub type MyDns<'a> = &'a mut dyn Dns<Error = wincwifi::StackError>;

#[allow(dead_code)]
pub enum ClientType {
    Tcp,
    Udp,
    Dns,
    UdpFull,
    TcpFull,
}

#[allow(dead_code)]
pub enum ReturnClient<'a> {
    Tcp(MyTcpClientStack<'a>),
    Udp(MyUdpClientStack<'a>),
    Dns(MyDns<'a>),
    UdpFull(MyUdpFullStack<'a>),
    TcpFull(MyTcpFullStack<'a>),
}

pub fn connect_and_run(
    message: &str,
    client_type: ClientType,
    execute: impl FnOnce(ReturnClient) -> Result<(), wincwifi::StackError>,
) -> Result<(), wincwifi::StackError> {
    if let Ok((delay_tick, mut red_led, cs, spi)) = init() {
        defmt::println!("{}", message);

        let mut countdown1 = MillisCountDown::new(&delay_tick);
        let mut countdown2 = MillisCountDown::new(&delay_tick);
        let mut countdown3 = MillisCountDown::new(&delay_tick);
        let mut delay_ms = create_delay_closure(&mut countdown1);
        let mut delay_ms2 = create_delay_closure(&mut countdown3);

        let mut stack = WincClient::new(
            SpiStream::new(cs, spi, create_delay_closure(&mut countdown2)),
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

        defmt::debug!("Chip started..");

        let ssid = option_env!("TEST_SSID").unwrap_or(DEFAULT_TEST_SSID);
        let password = option_env!("TEST_PASSWORD").unwrap_or(DEFAULT_TEST_PASSWORD);

        nb::block!(stack.connect_to_ap(ssid, password))?;

        defmt::info!("Network connected");
        for _ in 0..10 {
            delay_ms(50);
            stack.heartbeat()?;
        }
        defmt::info!("Running the demo..");
        match client_type {
            ClientType::Tcp => execute(ReturnClient::Tcp(&mut stack))?,
            ClientType::Udp => execute(ReturnClient::Udp(&mut stack))?,
            ClientType::Dns => execute(ReturnClient::Dns(&mut stack))?,
            ClientType::UdpFull => execute(ReturnClient::UdpFull(&mut stack))?,
            ClientType::TcpFull => execute(ReturnClient::TcpFull(&mut stack))?,
        }

        loop {
            stack.heartbeat()?;

            delay_ms(200u32);
            red_led.set_high()?;
            delay_ms(200u32);
            red_led.set_low()?;
        }
    }
    Ok(())
}
