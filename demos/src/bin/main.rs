use clap::Parser;
use core::net::Ipv4Addr;

use std_embedded_nal::Stack;

#[cfg(feature = "iperf3")]
use demos::iperf3_client::{iperf3_client, Conf, TestConfig};
use std::{str::FromStr, time::Instant};
#[cfg(feature = "iperf3")]
use std::{thread, time::Duration};

use demos::{
    coap_client::coap_client,
    http_client::http_client,
    http_server::http_server,
    http_speed_test::{
        speed_test, SpeedTestConfig, TEST_FILE_10MB, TEST_FILE_1MB, TEST_SERVER_HOST,
        TEST_SERVER_IP, TEST_SERVER_PORT,
    },
    tcp_server::tcp_server,
    telnet_shell::telnet_shell,
    udp_client::udp_client,
    udp_server::udp_server,
};

use log::Level;

pub fn parse_ip_octets(ip: &str) -> Result<[u8; 4], core::net::AddrParseError> {
    core::net::Ipv4Addr::from_str(ip).map(|addr| addr.octets())
}

#[derive(Clone, clap::Subcommand, Debug)]
enum Mode {
    UdpServer,
    UdpClient,
    HttpClient,
    TcpServer,
    CoapClient,
    HttpServer,
    HttpSpeedTest(HttpSpeedTestConfig),
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

#[derive(Parser, Clone, Debug)]
struct HttpSpeedTestConfig {
    /// Test file to download (1mb or 10mb)
    #[arg(short = 'f', long, default_value = "1mb")]
    file_size: String,

    /// Server to test against
    #[arg(short = 's', long)]
    server: Option<String>,

    /// Report progress every N receive calls
    #[arg(short = 'r', long, default_value_t = 32)]
    report_interval: u32,
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
    ParseError,
}

impl From<core::net::AddrParseError> for LocalErrors {
    fn from(_: core::net::AddrParseError) -> Self {
        LocalErrors::ParseError
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
    let ip = parse_ip_octets(&ip_str)?;
    let ip_addr = Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3]);
    let port = cli.port.unwrap_or(80);

    let mut stack = Stack;

    match cli.mode {
        Mode::HttpClient => {
            http_client(&mut stack, ip_addr, port, cli.hostname.as_deref())
                .map_err(|_| LocalErrors::TcpError)?;
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
        Mode::HttpSpeedTest(config) => {
            // Determine server IP and port
            let server_ip = if let Some(ref server) = config.server {
                parse_ip_octets(server)?
            } else {
                parse_ip_octets(TEST_SERVER_IP)?
            };
            let server_addr = Ipv4Addr::new(server_ip[0], server_ip[1], server_ip[2], server_ip[3]);
            let server_port = cli.port.unwrap_or(TEST_SERVER_PORT);

            // Determine test file
            let test_file = match config.file_size.as_str() {
                "10mb" => TEST_FILE_10MB,
                _ => TEST_FILE_1MB, // Default to 1MB
            };

            let test_config = SpeedTestConfig {
                server_host: config.server.as_deref().unwrap_or(TEST_SERVER_HOST),
                test_file,
                report_interval: config.report_interval,
            };

            println!("=== HTTP Speed Test ===");
            println!("Server: {} ({})", test_config.server_host, server_addr);
            println!("File: {test_file}");

            // Create timing function using std::time::Instant
            let start_time = Instant::now();
            let get_elapsed_seconds = || start_time.elapsed().as_secs_f32();

            match speed_test(
                &mut stack,
                server_addr,
                server_port,
                test_config,
                get_elapsed_seconds,
            ) {
                Ok(result) => {
                    println!("=== Test Results ===");
                    println!("Total downloaded: {} bytes", result.total_bytes);
                    println!("Time elapsed: {:.2} seconds", result.elapsed_seconds);
                    println!(
                        "Average speed: {:.1} KB/s ({:.1} Mbits/s)",
                        result.average_speed_kbps,
                        result.average_speed_kbps * 8.0 / 1024.0
                    );
                }
                Err(e) => {
                    eprintln!("Speed test failed: {e:?}");
                    return Err(LocalErrors::IoError);
                }
            }
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
