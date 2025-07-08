#![no_main]
#![no_std]

use bsp::shared::SpiStream;
use core::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use core::str::FromStr;
use embedded_nal::{UdpClientStack, UdpFullStack};
use feather as bsp;
use feather::hal::ehal::digital::OutputPin;
use feather::init::init;
use feather::shared::{create_countdowns, delay_fn};
use feather::{debug, error, info};

const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";
const DEFAULT_TEST_MDNS_PORT: &str = "5353";
const DEFAULT_TEST_MDNS_IP: &str = "224.0.0.251";

use wincwifi::{
    Credentials, SocketOptions, Ssid, StackError, UdpSockOpts, WifiChannel, WincClient,
};

fn program() -> Result<(), StackError> {
    if let Ok(mut ini) = init() {
        info!("Hello, Winc Module");

        let mut cnt = create_countdowns(&ini.delay_tick);
        let red_led = &mut ini.red_led;

        let mut delay_ms = delay_fn(&mut cnt.0);

        let ssid = Ssid::from(option_env!("TEST_SSID").unwrap_or(DEFAULT_TEST_SSID)).unwrap();
        let password = option_env!("TEST_PASSWORD").unwrap_or(DEFAULT_TEST_PASSWORD);
        let credentials = Credentials::from_wpa(password)?;
        info!(
            "Connecting to network: {} with password: {}",
            ssid.as_str(),
            password
        );
        let mut stack = WincClient::new(SpiStream::new(ini.cs, ini.spi));

        let mut v = 0;
        loop {
            match stack.start_wifi_module() {
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
            stack.heartbeat().unwrap();
            delay_ms(200);
        }

        info!("Started, connecting to AP ..");
        nb::block!(stack.connect_to_ap(&ssid, &credentials, WifiChannel::ChannelAll, false))?;

        debug!("Getting IP settings..");
        let info = nb::block!(stack.get_ip_settings())?;
        let ip = info.ip;

        info!(
            "Starting mDNS server at {}.{}.{}.{}:{}",
            ip.octets()[0],
            ip.octets()[1],
            ip.octets()[2],
            ip.octets()[3],
            DEFAULT_TEST_MDNS_PORT
        );
        for _ in 0..20 {
            delay_ms(100);
            stack.heartbeat()?;
        }

        // Create the UDP socket
        let mut socket = stack.socket()?;
        let test_port = option_env!("TEST_PORT").unwrap_or(DEFAULT_TEST_MDNS_PORT);
        let test_ip = option_env!("TEST_IP").unwrap_or(DEFAULT_TEST_MDNS_IP);
        let ip = Ipv4Addr::from_str(test_ip).map_err(|_| StackError::InvalidParameters)?;
        let port = u16::from_str(test_port).unwrap();
        // Set the Socket Options to multicast
        // Bind the Socket
        debug!("-----Binding to UDP port {}-----", port);
        stack.bind(&mut socket, port)?;
        info!("-----Bound to UDP port {}-----", port);

        info!("Server started listening");

        // This should probably done AFTER the bind
        let multicast_opt = SocketOptions::Udp(UdpSockOpts::JoinMulticast(ip));
        stack.set_socket_option(&mut socket, &multicast_opt)?;
        info!("Sent multicast join packet");

        let mut buffer = [0x0u8; 2048];

        // Fake mDNS query packet - no reason, just to test
        let craft_packet = [
            0x00, 0x00, // transaction
            0x00, 0x00, // standard query
            0x00, 0x01, // 1 question
            0x00, 0x00, // 0 answers
            0x00, 0x00, // 0 authority records
            0x00, 0x00, // 0 additional records
            0x08, b'_', b'b', b'r', b'r', b'd', b'i', b'n', b'o', 0x04, b'_', b't', b'c', b'p',
            0x05, b'l', b'o', b'c', b'a', b'l', 0x00, // termination
            0x00, 0x0c, 0x00, 0x01,
        ];
        let addr = SocketAddr::V4(SocketAddrV4::new(ip, port));
        nb::block!(stack.send_to(&mut socket, addr, &craft_packet))?;

        loop {
            delay_ms(200);
            red_led.set_high().unwrap();
            let (len, addr) = nb::block!(stack.receive(&mut socket, &mut buffer))?;
            if let SocketAddr::V4(addr) = addr {
                info!("Received: {:?}", len);
                info!(
                    "From: {}.{}.{}.{}:{}",
                    addr.ip().octets()[0],
                    addr.ip().octets()[1],
                    addr.ip().octets()[2],
                    addr.ip().octets()[3],
                    addr.port()
                );
                info!("Hex: {:#x}", &buffer[0..len]);
            } else {
                info!("Received: something else {:?}", len);
            }
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
        error!("Error: {}", err);
        panic!("Error in main program");
    } else {
        info!("Good exit")
    };
    loop {}
}
