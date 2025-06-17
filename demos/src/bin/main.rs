use clap::Parser;
use core::net::Ipv4Addr;

use std_embedded_nal::Stack;

#[cfg(feature = "iperf3")]
use demos::iperf3_client::{iperf3_client, Conf, TestConfig};
#[cfg(feature = "iperf3")]
use std::{thread, time::Duration};

use demos::{
    coap_client::coap_client, http_client::http_client, http_server::http_server,
    tcp_server::tcp_server, telnet_shell::telnet_shell, udp_client::udp_client,
    udp_server::udp_server,
};

use log::Level;

pub fn parse_ip_octets(ip: &str) -> [u8; 4] {
    let mut octets = [0; 4];
    let mut octet_index = 0;
    let mut current_value = 0;

    ip.bytes().for_each(|byte| match byte {
        b'0'..=b'9' => current_value = current_value * 10 + (byte - b'0'),
        b'.' => {
            octets[octet_index] = current_value;
            octet_index += 1;
            current_value = 0;
        }
        _ => {}
    });

    octets[octet_index] = current_value;
    octets
}

#[derive(Clone, clap::Subcommand, Debug)]
enum Mode {
    UdpServer,
    UdpClient,
    HttpClient,
    TcpServer,
    CoapClient,
    HttpServer,
    Iperf3Client(Iperf3Config), // Embed the config directly in the mode
    TelnetServer,
}

#[derive(Parser, Clone, Debug)]
struct Iperf3Config {
    /// number of bytes to transmit (instead of -t)
    #[arg(short, default_value_t = 32)]
    numbytes: usize,

    /// time in seconds to transmit for (default 10 secs)
    #[arg(short = 't', long = "time", default_value_t = 10)]
    time: usize,

    /// number of blocks (packets) to transmit (instead of -t or -n)
    #[arg(short = 'k', long)]
    numblocks: Option<usize>,

    /// length of buffer to read or write
    #[arg(short = 'l', long, default_value_t = 32)]
    block_len: usize,

    /// use UDP rather than TCP
    #[arg(short = 'u', long)]
    udp: bool,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    mode: Mode,

    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,

    #[arg(short, long)]
    ip: Option<String>,

    #[arg(short, long)]
    port: Option<u16>,

    #[arg(short = 'o', long)]
    hostname: Option<String>,

    #[arg(short, long, default_value = "false")]
    loop_forever: bool,
}

#[derive(Debug)]
enum LocalErrors {
    TcpError,
    IoError,
}

impl<E: embedded_nal::TcpError> From<E> for LocalErrors {
    fn from(_: E) -> Self {
        LocalErrors::TcpError
    }
}

fn main() -> Result<(), LocalErrors> {
    let cli = Cli::parse();

    let log_level = match cli.debug {
        1 => Level::Info,
        2 => Level::Debug,
        3 => Level::Trace,
        _ => Level::Warn,
    };
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(log_level.to_string()),
    )
    .init();
    log::info!("Starting embedded-nal demo application");

    let ip_str = cli.ip.unwrap_or("127.0.0.1".to_string());
    let ip = parse_ip_octets(&ip_str);
    let ip_addr = Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3]);
    let port = cli.port.unwrap_or(80);

    let mut stack = Stack::default();

    match cli.mode {
        Mode::HttpClient => {
            http_client(&mut stack, ip_addr, port, cli.hostname.as_deref())?;
        }
        Mode::UdpServer => {
            udp_server(&mut stack, port, cli.loop_forever).map_err(|_| LocalErrors::IoError)?;
        }
        Mode::UdpClient => {
            udp_client(&mut stack, ip_addr, port).map_err(|_| LocalErrors::IoError)?;
        }
        Mode::TcpServer => {
            tcp_server(&mut stack, port, cli.loop_forever).map_err(|_| LocalErrors::IoError)?;
        }

        Mode::CoapClient => {
            coap_client(&mut stack, ip_addr, port).map_err(|_| LocalErrors::IoError)?;
        }
        Mode::HttpServer => {
            http_server(&mut stack, port).map_err(|_| LocalErrors::IoError)?;
        }
        Mode::Iperf3Client(_config) => {
            #[cfg(feature = "iperf3")]
            {
                let conf = if _config.numblocks.is_some() {
                    TestConfig {
                        conf: Conf::Blocks(_config.numblocks.unwrap()),
                        transmit_block_len: _config.block_len,
                    }
                } else {
                    TestConfig {
                        conf: Conf::Bytes(_config.numbytes),
                        transmit_block_len: _config.block_len,
                    }
                };
                let mut delay_ms = |ms: u32| {
                    thread::sleep(Duration::from_millis(ms as u64));
                };
                iperf3_client::<65536, _, _, _>(
                    &mut stack,
                    ip_addr,
                    Some(port),
                    &mut rand::rng(),
                    Some(conf),
                    _config.udp, // Pass UDP flag directly
                    &mut delay_ms,
                )
                .map_err(|_| LocalErrors::IoError)?;
            }
        }
        #[cfg(feature = "telnet")]
        Mode::TelnetServer => {
            telnet_shell(&mut stack, cli.port).map_err(|_| LocalErrors::IoError)?;
        }
    }
    Ok(())
}
