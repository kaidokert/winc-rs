//! Bypass Mode Example

#![no_main]
#![no_std]

mod ext_tcp_stack;

use core::net::Ipv4Addr;
use core::str::FromStr;

use bsp::shared::SpiStream;
use feather as bsp;
use feather::hal::ehal::digital::OutputPin;
use feather::init::init;
use feather::shared::{create_countdowns, delay_fn};
use feather::{debug, error, info};
use rand_core::RngCore;
use smoltcp::iface::SocketStorage;

use ext_tcp_stack::{Stack, TcpStackError};
use wincwifi::{CommError, Credentials, Ssid, StackError, WifiChannel, WincClient};

const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";
const DEFAULT_TEST_IP: &str = "8.8.8.8";
const DEFAULT_TEST_COUNT: &str = "4";

fn program() -> Result<(), StackError> {
    if let Ok(mut ini) = init() {
        info!("Hello, Winc Ethernet Module");

        let mut cnt = create_countdowns(&ini.delay_tick);
        let mut delay_ms = delay_fn(&mut cnt.0);

        let red_led = &mut ini.red_led;

        let test_ip = option_env!("TEST_IP").unwrap_or(DEFAULT_TEST_IP);
        let test_ip: Ipv4Addr =
            Ipv4Addr::from_str(test_ip).map_err(|_| StackError::InvalidParameters)?;
        let test_count = option_env!("TEST_COUNT").unwrap_or(DEFAULT_TEST_COUNT);
        let test_count = u16::from_str(test_count).unwrap();

        let ssid = Ssid::from(option_env!("TEST_SSID").unwrap_or(DEFAULT_TEST_SSID)).unwrap();
        let password = option_env!("TEST_PASSWORD").unwrap_or(DEFAULT_TEST_PASSWORD);
        let credentials = Credentials::from_wpa(password)?;

        info!(
            "Connecting to network: {} with password: {}",
            ssid.as_str(),
            password
        );

        let mut device = WincClient::new(SpiStream::new(ini.cs, ini.spi));

        let mut v = 0;
        loop {
            match device.start_in_ethernet_mode() {
                Ok(_) => break,
                Err(nb::Error::WouldBlock) => {
                    debug!("Waiting start .. {}", v);
                    v += 1;
                    delay_ms(5)
                }
                Err(e) => return Err(e.into()),
            }
        }

        for _ in 0..20 {
            device.heartbeat()?;
            delay_ms(200);
        }

        info!("Started, connecting to AP ..");
        nb::block!(device.connect_to_ap(&ssid, &credentials, WifiChannel::ChannelAll, false))?;

        let random_seed = device.next_u64();
        // Get the MAC Address of WINC device.
        let mac = device.get_winc_mac_address()?;

        // Init TCP/IP Stack
        let mut sock_storage = [SocketStorage::EMPTY; 3];
        let mut stack = Stack::new(
            &mut device,
            random_seed,
            &mut sock_storage,
            mac.octets(),
            &mut cnt.1,
        )
        .map_err(|e| {
            error!("TCP stack initialization failed: {:?}", e);
            StackError::WincWifiFail(CommError::Failed)
        })?;

        // Acquire IP from DHCP.
        stack.config_v4().map_err(|e| {
            error!("Failed to acquire IP from DHCP: {:?}", e);

            if e == TcpStackError::Timeout {
                StackError::GeneralTimeout
            } else {
                StackError::WincWifiFail(CommError::Failed)
            }
        })?;

        // ping server
        stack.send_ping(test_ip, test_count).map_err(|e| {
            error!("Failed to ping server: {:?}", e);

            if e == TcpStackError::Timeout {
                StackError::GeneralTimeout
            } else {
                StackError::WincWifiFail(CommError::Failed)
            }
        })?;

        loop {
            delay_ms(200);
            red_led.set_high().unwrap();
            delay_ms(200);
            red_led.set_low().unwrap();
            device.heartbeat()?;
        }
    }
    Ok(())
}

#[cortex_m_rt::entry]
fn main() -> ! {
    if let Err(err) = program() {
        error!("Error: {}", err);
        panic!("Error in main program");
    } else {
        info!("Good exit")
    };
    loop {}
}
