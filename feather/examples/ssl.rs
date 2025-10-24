//! SSL example.

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
    CommError, Credentials, SocketOptions, Ssid, SslCipherSuite, SslSockConfig, StackError,
    WifiChannel, WincClient,
};

const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";
const DEFAULT_TEST_HOST: &str = "dhe-rsa-gcm128.ssltest.coapbin.org";
const DEFAULT_TEST_SSL_PORT: &str = "443";

/// HTTP response parsing errors.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Eq)]
enum Error {
    EmptyResponse,
    IncompleteResponse,
    DataCorrupted,
    InvalidStatus(u16),
}

/// Parse the HTTP response.
///
/// # Arguments
///
/// `response` - The HTTP response received from the server.
fn parse_http_response(response: &str) -> Result<(), Error> {
    // Valid HTTP code.
    const HTTP_OK: u16 = 200;
    // split the response at line endings.
    let mut resp_lines = response.lines();

    // 1. Parse status line
    let status_line = match resp_lines.next() {
        Some(line) => line,
        None => return Err(Error::EmptyResponse),
    };

    // a. split the status line in three parts from space.
    // e.g: "HTTP/1.1 200 OK" -> ["HTTP/1.1", "200", "OK"]
    let mut status_parts = status_line.splitn(3, ' ');

    // b. Skip HTTP version
    let _ = status_parts.next();

    // c. Parse status code
    let status_code: u16 = status_parts
        .next()
        .ok_or(Error::IncompleteResponse)?
        .parse()
        .map_err(|_| Error::DataCorrupted)?;

    if status_code != HTTP_OK {
        return Err(Error::InvalidStatus(status_code));
    }

    // 2. Skip headers till blank line ("\r\n").
    for line in &mut resp_lines {
        if line.trim().is_empty() {
            break;
        }
    }

    // 3. Check if a message body is present.
    let mut body_lines = resp_lines.peekable();
    if body_lines.peek().is_none() {
        return Err(Error::IncompleteResponse);
    } else {
        info!("-> HTTP Response: {}", status_code);

        // 4. Print the body.
        for line in body_lines {
            info!("-> {}", line);
        }
    }

    Ok(())
}

fn program() -> Result<(), StackError> {
    if let Ok(mut ini) = init() {
        info!("Hello, Winc SSL Module");

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

        // set cipher suite
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

        // Sending HTTP request
        info!("Sending HTTP request!");
        let mut http_get_buf = [0u8; 128];
        let request = {
            let base = b"GET / HTTP/1.1\r\nHost: ";
            let middle = b"\r\nUser-Agent: winc-rs/0.2.2\r\nAccept: */*\r\n\r\n";
            let mut pos = 0;
            let header_len = base.len() + host.as_bytes().len() + middle.len();
            if header_len > http_get_buf.len() {
                error!("HTTP request buffer is not sufficient to store the HTTP header.");
                return Err(StackError::InvalidParameters);
            }

            http_get_buf[..base.len()].copy_from_slice(base);
            pos += base.len();

            let host_bytes = host.as_bytes();
            http_get_buf[pos..pos + host_bytes.len()].copy_from_slice(host_bytes);
            pos += host_bytes.len();

            http_get_buf[pos..pos + middle.len()].copy_from_slice(middle);
            pos += middle.len();

            &http_get_buf[..pos]
        };

        nb::block!(stack.send(&mut socket, request))?;

        // receiving okay
        info!("Waiting for response!");
        let mut resp_buffer = [0u8; 300];
        let rcv_len = nb::block!(stack.receive(&mut socket, &mut resp_buffer))?;

        let str_response = core::str::from_utf8(&resp_buffer[..rcv_len])
            .map_err(|err| StackError::WincWifiFail(CommError::Utf8Error(err)))?;

        info!("Response received:");

        let result = parse_http_response(str_response);

        if let Err(err) = result {
            match err {
                Error::InvalidStatus(code) => {
                    error!("-> HTTP request failed with error: {}", code);
                }
                _ => {
                    error!("-> Parsing HTTP response failed with error {}", err);
                }
            }

            return Err(StackError::InvalidResponse);
        }

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
