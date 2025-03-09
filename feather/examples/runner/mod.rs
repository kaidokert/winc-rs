use embedded_nal::{Dns, TcpClientStack, TcpFullStack, UdpClientStack, UdpFullStack};
use feather::hal::ehal::digital::OutputPin;
use feather::{
    init::init,
    shared::{create_countdowns, delay_fn, SpiStream},
};

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
    execute: impl FnOnce(ReturnClient, core::net::Ipv4Addr) -> Result<(), wincwifi::StackError>,
) -> Result<(), wincwifi::StackError> {
    if let Ok(mut ini) = init() {
        defmt::println!("{}", message);

        let mut cnt = create_countdowns(&ini.delay_tick);
        let red_led = &mut ini.red_led;

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

        defmt::debug!("Chip started..");

        let ssid = option_env!("TEST_SSID").unwrap_or(DEFAULT_TEST_SSID);
        let password = option_env!("TEST_PASSWORD").unwrap_or(DEFAULT_TEST_PASSWORD);

        for _ in 0..10 {
            delay_ms(50);
            stack.heartbeat()?;
        }
        defmt::debug!("Connecting to AP.. {} {}", ssid, password);
        nb::block!(stack.connect_to_ap(ssid, password, false))?;

        defmt::debug!("Getting IP settings..");
        let info = nb::block!(stack.get_ip_settings())?;
        let my_ip = info.ip;
        for _ in 0..10 {
            delay_ms(50);
            stack.heartbeat()?;
        }
        defmt::info!("Running the demo..");
        match client_type {
            ClientType::Tcp => execute(ReturnClient::Tcp(&mut stack), my_ip)?,
            ClientType::Udp => execute(ReturnClient::Udp(&mut stack), my_ip)?,
            ClientType::Dns => execute(ReturnClient::Dns(&mut stack), my_ip)?,
            ClientType::UdpFull => execute(ReturnClient::UdpFull(&mut stack), my_ip)?,
            ClientType::TcpFull => execute(ReturnClient::TcpFull(&mut stack), my_ip)?,
        }

        loop {
            stack.heartbeat()?;

            delay_ms(200u32);
            red_led.set_high().unwrap();
            delay_ms(200u32);
            red_led.set_low().unwrap();
        }
    }
    Ok(())
}
