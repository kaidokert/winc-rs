use cortex_m_systick_countdown::MillisCountDown;
use embedded_nal::{TcpClientStack, UdpClientStack};
use feather::{init::init, shared::{create_delay_closure, SpiStream}};
use wincwifi::manager::{AuthType, EventListener, Manager};

use super::bsp::hal::prelude::*;
use wincwifi::Handle;

use crate::{stack::WincClient, DEFAULT_TEST_PASSWORD, DEFAULT_TEST_SSID};

pub type MyTcpClientStack<'a> = &'a mut dyn TcpClientStack<TcpSocket = Handle, Error = crate::stack::StackError>;

pub type MyUdpClientStack<'a> = &'a mut dyn UdpClientStack<UdpSocket = Handle, Error = crate::stack::StackError>;

pub struct Callbacks {
    connected: bool
}
impl EventListener for Callbacks {
    fn on_dhcp(&mut self, conf: wincwifi::manager::IPConf) {
        defmt::info!("Network connected: IP config: {}", conf);
        self.connected = true;
    }
    fn on_connstate_changed(&mut self, state: wincwifi::manager::WifiConnState, err: wincwifi::manager::WifiConnError) {
        defmt::info!("Connection state changed: {:?} {:?}", state, err);
    }
    fn on_system_time(&mut self, year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) {
        defmt::info!(
            "System time received: {}-{:02}-{:02} {:02}:{:02}:{:02}",
            year,
            month,
            day,
            hour,
            minute,
            second
        );
    }
}

pub fn connect_and_run(message: &str, tcp: bool,
    execute_tcp: impl FnOnce(MyTcpClientStack) -> Result<(), crate::stack::StackError>,
    execute_udp: impl FnOnce(MyUdpClientStack) -> Result<(), crate::stack::StackError>,
) -> Result<(), crate::stack::StackError> {
    if let Ok((delay_tick, mut red_led, cs, spi)) = init() {
        defmt::println!("{}", message);

        let mut countdown1 = MillisCountDown::new(&delay_tick);
        let mut countdown2 = MillisCountDown::new(&delay_tick);
        let mut countdown3 = MillisCountDown::new(&delay_tick);
        let mut delay_ms = create_delay_closure(&mut countdown1);
        let mut delay_ms2 = create_delay_closure(&mut countdown3);

        let mut manager = Manager::from_xfer(
            SpiStream::new(cs, spi, create_delay_closure(&mut countdown2)),
            Callbacks { connected: false },
        );
        manager.set_crc_state(true);

        manager.start(&mut |v: u32| -> bool {
            defmt::debug!("Waiting start .. {}", v);
            delay_ms(40);
            false
        })?;
        defmt::debug!("Chip started..");

        let ssid = option_env!("TEST_SSID").unwrap_or(DEFAULT_TEST_SSID);
        let password = option_env!("TEST_PASSWORD").unwrap_or(DEFAULT_TEST_PASSWORD);

        manager.send_connect(AuthType::WpaPSK, ssid, password, 0xFF, false)?;

        for _ in 0..10 {
            manager.dispatch_events()?;
            delay_ms(300u32);
            if manager.listener.connected {
                break;
            }
        }
        let connected = manager.listener.connected;
        let mut stack = WincClient::new(manager, &mut delay_ms2);
        if connected {
            defmt::info!("Network connected");
            if tcp {
                execute_tcp(&mut stack)?;
            } else {
                defmt::info!("Call UDP here ..");
                execute_udp(&mut stack)?;
            }
        } else {
            defmt::error!("Failed to connect to network");
        }
        loop {
            stack.dispatch_events()?;

            delay_ms(200u32);
            red_led.set_high()?;
            delay_ms(200u32);
            red_led.set_low()?;
        }
    }
    Ok(())
}
