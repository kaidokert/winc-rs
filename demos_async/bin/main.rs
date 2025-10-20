use clap::Parser;
use std::net::Ipv4Addr;
use std::str::FromStr;

use demos_async::udp_client;
use embedded_nal_async::UdpStack;
use std_embedded_nal_async::Stack;

#[derive(Clone, clap::Subcommand, Debug)]
enum Mode {
    UdpClient,
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
}

#[derive(Debug)]
enum LocalErrors {
    IoError(String),
    ParseError(String),
}

impl From<std::net::AddrParseError> for LocalErrors {
    fn from(e: std::net::AddrParseError) -> Self {
        LocalErrors::ParseError(format!("Address parse error: {}", e))
    }
}

impl From<Box<dyn std::error::Error>> for LocalErrors {
    fn from(e: Box<dyn std::error::Error>) -> Self {
        LocalErrors::IoError(e.to_string())
    }
}

fn main() -> Result<(), LocalErrors> {
    let cli = Cli::parse();

    let log_level = match cli.debug {
        1 => log::Level::Info,
        2 => log::Level::Debug,
        3 => log::Level::Trace,
        _ => log::Level::Warn,
    };
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(log_level.to_string()),
    )
    .init();

    log::info!("Starting embedded-nal-async demo application");

    let ip_str = cli.ip.unwrap_or("127.0.0.1".to_string());
    let ip_addr = Ipv4Addr::from_str(&ip_str)?;
    let port = cli.port.unwrap_or(8080);

    let mut stack = Stack::default();

    match cli.mode {
        Mode::UdpClient => {
            // Run async UDP client using smol runtime
            smol::block_on(async {
                // Bind to local address (0.0.0.0:0 for auto-assign)
                let local_addr =
                    std::net::SocketAddr::V4(std::net::SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0));
                let (bound_addr, mut socket) = stack
                    .bind_single(local_addr)
                    .await
                    .map_err(|e| LocalErrors::IoError(format!("Failed to bind socket: {}", e)))?;

                let mut recv_buffer = [0u8; 1024];
                let recv_len = udp_client::run_udp_client(
                    &mut socket,
                    bound_addr,
                    ip_addr,
                    port,
                    b"Hello, UDP!",
                    &mut recv_buffer,
                )
                .await
                .map_err(|e| LocalErrors::IoError(e.to_string()))?;

                log::info!(
                    "Received {} bytes: {:?}",
                    recv_len,
                    &recv_buffer[..recv_len]
                );
                Ok::<(), LocalErrors>(())
            })?;
        }
    }

    Ok(())
}
