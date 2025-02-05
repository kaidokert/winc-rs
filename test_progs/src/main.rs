use clap::Parser;
use core::net::Ipv4Addr;

use std_embedded_nal::Stack;

use demos::{http_client::http_client, tcp_server::tcp_server, udp_server::udp_server};

use log::{debug, error, info, Level};

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

#[derive(Clone, clap::ValueEnum, Debug)]
enum Mode {
    UdpServer,
    UdpClient,
    HttpClient,
    TcpServer,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long, default_value = "http-client")]
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
        Mode::TcpServer => {
            tcp_server(&mut stack, port, cli.loop_forever).map_err(|_| LocalErrors::IoError)?;
        }
        _ => {}
    }
    Ok(())
}
