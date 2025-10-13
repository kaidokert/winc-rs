#![no_main]
#![no_std]

use bsp::shared::SpiStream;
use core::net::SocketAddr;
use core::str::FromStr;
use embedded_nal::{Dns, TcpClientStack};
use feather as bsp;
use feather::hal::ehal::digital::OutputPin;
use feather::init::init;
use feather::shared::{create_countdowns, delay_fn};
use feather::{debug, error, info};
use wincwifi::{
    Credentials, SocketOptions, Ssid, SslSockConfig, StackError, WifiChannel, WincClient, SslCipherSuite
};

const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";
const DEFAULT_TEST_HOST: &str = "example.org";
const DEFAULT_TEST_SSL_PORT: &str = "443";

fn program() -> Result<(), StackError> {
    if let Ok(mut ini) = init() {
        info!("Hello, SSL");

        let mut cnt = create_countdowns(&ini.delay_tick);
        let red_led = &mut ini.red_led;

        let mut delay_ms = delay_fn(&mut cnt.0);

        let host = option_env!("TEST_HOST").unwrap_or(DEFAULT_TEST_HOST);
        let port_str = option_env!("TEST_PORT").unwrap_or(DEFAULT_TEST_SSL_PORT);
        let ssid = Ssid::from(option_env!("TEST_SSID").unwrap_or(DEFAULT_TEST_SSID)).unwrap();
        let password = option_env!("TEST_PASSWORD").unwrap_or(DEFAULT_TEST_PASSWORD);
        let credentials = Credentials::from_wpa(password)?;
        info!(
            "Connecting to network: {} with password: {}",
            ssid.as_str(),
            password
        );
        info!("Target host: '{}' port: '{}'", host, port_str);
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

        // set cipher suit
        nb::block!(stack.ssl_set_cipher_suite(SslCipherSuite::AllCiphers))?;

        info!("Started, connecting to AP ..");
        nb::block!(stack.connect_to_ap(&ssid, &credentials, WifiChannel::ChannelAll, false))?;

        // wait for DHCP to do its magic.
        nb::block!(stack.get_ip_settings())?;

        // resolve the host
        let ip = nb::block!(stack.get_host_by_name(host, embedded_nal::AddrType::IPv4))?;
        // socket address
        let port = u16::from_str(port_str).unwrap();
        let addr = SocketAddr::new(ip, port);

        // Create the TCP socket
        let mut socket = stack.socket()?;
        // enable ssl on socket
        let ssl_sock = SocketOptions::config_ssl(SslSockConfig::EnableSSL, true);
        stack.set_socket_option(&mut socket, &ssl_sock)?;

        // set sni
        let sni = SocketOptions::set_sni(host)?;
        stack.set_socket_option(&mut socket, &sni)?;

        // connect with server
        nb::block!(stack.connect(&mut socket, addr))?;
        info!("Connected with Server");

        // Build and send HTTP GET request with Host header
        let mut http_get_buf = [0u8; 256];
        let base = b"GET / HTTP/1.1\r\nHost: ";
        let suffix = b"\r\n\r\n                            ";
        let mut pos = 0;

        http_get_buf[..base.len()].copy_from_slice(base);
        pos += base.len();

        let host_bytes = host.as_bytes();
        http_get_buf[pos..pos + host_bytes.len()].copy_from_slice(host_bytes);
        pos += host_bytes.len();

        http_get_buf[pos..pos + suffix.len()].copy_from_slice(suffix);
        pos += suffix.len();

        let http_get = &http_get_buf[..pos];

        // Debug: print the actual request
        if let Ok(req_str) = core::str::from_utf8(http_get) {
            info!("Sending request: '{}'", req_str);
        }

        // Send the HTTP request
        let nbytes = nb::block!(stack.send(&mut socket, http_get))?;
        info!("Request sent {} bytes", nbytes);

        // Receive the response
        let mut respbuf = [0; 1500];
        let resplen = nb::block!(stack.receive(&mut socket, &mut respbuf))?;
        info!("Response received {} bytes", resplen);

        // Parse and display the response
        let the_received_slice = &respbuf[..resplen];
        if let Ok(recvd_str) = core::str::from_utf8(the_received_slice) {
            info!("Response: {}", recvd_str);
        } else {
            info!("Response contains non-UTF8 data");
        }

        // Close the socket
        stack.close(socket)?;
        info!("Socket closed");

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
        error!("Error: {}", err);
        panic!("Error in main program");
    } else {
        info!("Good exit")
    };
    loop {}
}
