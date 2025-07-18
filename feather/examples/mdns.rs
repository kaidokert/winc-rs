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
const MAX_SIZE_MDNS_SERVICE_NAME: usize = 64;
const MAX_MDNS_RESPONSE_SIZE: usize = 185;
const MDNS_HEADER_SIZE: usize = 12;
const MDNS_SERVICE_NAME: &str = "_brrdino._tcp.local";

use wincwifi::{Credentials, SocketOptions, Ssid, StackError, WifiChannel, WincClient};

fn parse_query(buffer: &[u8], service_name: &str) -> bool {
    // check if header (12 bytes) is valid
    if buffer.len() < MDNS_HEADER_SIZE {
        error!("buffer too short");
        return false;
    }

    // check number of questions
    let qdcount = u16::from_be_bytes([buffer[4], buffer[5]]);
    if qdcount == 0 {
        error!("qdcount is 0");
        return false;
    }

    // Check for supported service name length
    if service_name.len() > MAX_SIZE_MDNS_SERVICE_NAME {
        error!("service name too long");
        return false;
    }

    let mut offset = MDNS_HEADER_SIZE;
    let mut qname = [0u8; MAX_SIZE_MDNS_SERVICE_NAME]; // buffer for parsed name
    let mut qname_index = 0;

    while offset < buffer.len() {
        let len = buffer[offset] as usize;

        if len == 0 {
            offset += 1;
            break;
        }

        // name compression is not supported.
        if len & 0xC0 != 0 {
            error!("name compression is not supported");
            return false;
        }

        offset += 1;
        if offset + len > buffer.len() || qname_index + len + 1 > qname.len() {
            error!("buffer too short");
            return false;
        }

        // copy label
        qname[qname_index..qname_index + len].copy_from_slice(&buffer[offset..offset + len]);
        qname_index += len;
        offset += len;

        // add dot separator
        qname[qname_index] = b'.';
        qname_index += 1;
    }

    // remove trailing dot if present
    if qname_index > 0 && qname[qname_index - 1] == b'.' {
        qname_index -= 1;
    }

    let parsed_name = &qname[..qname_index];
    let expected_name = service_name.as_bytes();

    // compare
    if parsed_name != expected_name {
        error!(
            "parsed name \n\t{:?} \ndoes not match expected name \n\t{:?}",
            core::str::from_utf8(parsed_name).unwrap(),
            core::str::from_utf8(expected_name).unwrap()
        );
        return false;
    }

    // Check the QTYPE (2 bytes) and QCLASS (2 bytes)
    if offset + 4 > buffer.len() {
        error!("buffer too short");
        return false;
    }

    matches!(
        (
            u16::from_be_bytes([buffer[offset], buffer[offset + 1]]),
            u16::from_be_bytes([buffer[offset + 2], buffer[offset + 3]])
        ),
        (0x000C, 0x0001) // QTYPE = PTR, QCLASS = IN
    )
}

fn build_response() -> [u8; MAX_MDNS_RESPONSE_SIZE] {
    let response_packet = [
        // --- Header
        0x00, 0x00, // transaction
        0x84, 0x00, // response, authoritative answer
        0x00, 0x00, // 0 question
        0x00, 0x02, // 2 answers
        0x00, 0x00, // 0 authority records
        0x00, 0x01, // 1 additional records
        // -- PTR record
        // service name "_brrdino._tcp.local"
        0x08, b'_', b'b', b'r', b'r', b'd', b'i', b'n', b'o', 0x04, b'_', b't', b'c', b'p', 0x05,
        b'l', b'o', b'c', b'a', b'l', 0x00, // termination
        0x00, 0x0c, 0x00, 0x01, // // QTYPE = PTR, QCLASS = IN
        0x00, 0x00, 0x00, 0x78, // TTL = 120
        0x00, 0x23, // RD Length: 35 bytes
        // --- PTR Data: "feather-board._brrdino._tcp.local"
        0x0D, b'f', b'e', b'a', b't', b'h', b'e', b'r', b'-', b'b', b'o', b'a', b'r', b'd', 0x08,
        b'_', b'b', b'r', b'r', b'd', b'i', b'n', b'o', 0x04, b'_', b't', b'c', b'p', 0x05, b'l',
        b'o', b'c', b'a', b'l', 0x00, // termination
        // --- Service Record
        // instance name "feather-board._brrdino._tcp.local"
        0x0D, b'f', b'e', b'a', b't', b'h', b'e', b'r', b'-', b'b', b'o', b'a', b'r', b'd', 0x08,
        b'_', b'b', b'r', b'r', b'd', b'i', b'n', b'o', 0x04, b'_', b't', b'c', b'p', 0x05, b'l',
        b'o', b'c', b'a', b'l', 0x00, // termination
        0x00, 0x21, 0x00, 0x01, // Type = SRV (33), Class = IN (1)
        0x00, 0x00, 0x00, 0x78, // TTL = 120
        0x00, 0x1B, // RD Length: 27 bytes
        0x00, 0x00, 0x00, 0x00, // Priority and Weight
        0x1F, 0x90, // Port 8080
        // hostname "feather-board.local"
        0x0D, b'f', b'e', b'a', b't', b'h', b'e', b'r', b'-', b'b', b'o', b'a', b'r', b'd', 0x05,
        b'l', b'o', b'c', b'a', b'l', 0x00, // termination
        // --- Additional Record
        0x0D, b'f', b'e', b'a', b't', b'h', b'e', b'r', b'-', b'b', b'o', b'a', b'r', b'd', 0x05,
        b'l', b'o', b'c', b'a', b'l', 0x00, // Termination
        0x00, 0x01, 0x00, 0x01, // Type = A, Class = IN
        0x00, 0x00, 0x00, 0x78, // TTL = 120
        0x00, 0x04, // RDLENGTH = 4 bytes
        0xC0, 0xA8, 0x01, 0x01, // IP = 192.168.1.1 (example)
    ];

    return response_packet;
}

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

        // Creat the UDP socket
        let mut socket = stack.socket()?;
        let test_port = option_env!("TEST_PORT").unwrap_or(DEFAULT_TEST_MDNS_PORT);
        let test_ip = option_env!("TEST_IP").unwrap_or(DEFAULT_TEST_MDNS_IP);
        let ip = Ipv4Addr::from_str(test_ip).map_err(|_| StackError::InvalidParameters)?;
        let port = u16::from_str(test_port).unwrap();
        // Set the Socket Options to multicast
        let multicast_opt = SocketOptions::join_multicast_v4(ip);

        // Bind the Socket
        debug!("-----Binding to UDP port {}-----", port);
        stack.bind(&mut socket, port)?;
        info!("-----Bound to UDP port {}-----", port);
        // Set the Socket Options to multicast
        stack.set_socket_option(&mut socket, &multicast_opt)?;
        info!("Server started listening");

        let mut buffer = [0x0u8; 4096];
        let mdns_addr = SocketAddr::V4(SocketAddrV4::new(ip, port));

        info!("--> Waiting for new multicast query");

        loop {
            delay_ms(200);
            red_led.set_high().unwrap();
            let (_, addr) = nb::block!(stack.receive(&mut socket, &mut buffer))?;
            if let SocketAddr::V4(addr) = addr {
                if parse_query(&buffer, MDNS_SERVICE_NAME) {
                    info!(
                        "Received query from: {}.{}.{}.{}:{}",
                        addr.ip().octets()[0],
                        addr.ip().octets()[1],
                        addr.ip().octets()[2],
                        addr.ip().octets()[3],
                        addr.port()
                    );
                    let res = build_response();
                    nb::block!(stack.send_to(&mut socket, mdns_addr, &res))?;
                    info!("<--- Sent multicast response packet");
                    info!("--> Waiting for new multicast query");
                } else {
                    info!(
                        "packet from addr {}.{}.{}.{}:{} not accepted",
                        addr.ip().octets()[0],
                        addr.ip().octets()[1],
                        addr.ip().octets()[2],
                        addr.ip().octets()[3],
                        addr.port()
                    );
                }
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
