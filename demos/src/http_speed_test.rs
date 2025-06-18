//! HTTP Speed Test - Download large file and measure throughput
//!
//! Downloads test files to measure network performance
//! Equivalent to the Arduino WifiSpeedTest for comparison

use super::{debug, error, info};
use core::net::{IpAddr, Ipv4Addr, SocketAddr};
use embedded_nal::nb::block;
use embedded_nal::TcpClientStack;

// Test server configuration
pub const TEST_SERVER_IP: &str = "18.155.192.71"; // kaidokert.com IP (AWS)
pub const TEST_SERVER_PORT: u16 = 80;
pub const TEST_SERVER_HOST: &str = "kaidokert.com";

// Test file options
pub const TEST_FILE_1MB: &str = "/test-file-1mb.json"; // 0.93 MB
pub const TEST_FILE_10MB: &str = "/test-file-10mb.json"; // 9.37 MB

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SpeedTestError {
    SocketCreation,
    NetworkError,
}

pub struct SpeedTestConfig<'a> {
    pub server_host: &'a str,
    pub test_file: &'a str,
    pub report_interval: u32,
}

impl<'a> Default for SpeedTestConfig<'a> {
    fn default() -> Self {
        Self {
            server_host: TEST_SERVER_HOST,
            test_file: TEST_FILE_1MB,
            report_interval: 32,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug)]
struct WrapError(httparse::Error);

#[cfg(feature = "defmt")]
impl defmt::Format for WrapError {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "<later>")
    }
}

pub struct SpeedTestResult {
    pub total_bytes: u32,
    pub elapsed_seconds: f32,
    pub average_speed_kbps: f32,
}

pub fn speed_test<T, S>(
    stack: &mut T,
    addr: Ipv4Addr,
    port: u16,
    config: SpeedTestConfig,
    get_elapsed_seconds: impl Fn() -> f32,
) -> Result<SpeedTestResult, SpeedTestError>
where
    T: TcpClientStack<TcpSocket = S> + ?Sized,
{
    let sock = stack.socket().map_err(|_| SpeedTestError::SocketCreation)?;
    let mut s = sock;

    info!(
        "Connecting to {}.{}.{}.{}:{}",
        addr.octets()[0],
        addr.octets()[1],
        addr.octets()[2],
        addr.octets()[3],
        port
    );

    let remote = SocketAddr::new(IpAddr::V4(addr), port);
    block!(stack.connect(&mut s, remote)).map_err(|_| SpeedTestError::NetworkError)?;

    info!("Connected to server");

    // Build HTTP GET request dynamically
    let mut request_buffer = [0u8; 256];
    let request_str = "GET ";
    let mut pos = 0;

    // Copy "GET "
    request_buffer[pos..pos + request_str.len()].copy_from_slice(request_str.as_bytes());
    pos += request_str.len();

    // Copy the test file path
    request_buffer[pos..pos + config.test_file.len()].copy_from_slice(config.test_file.as_bytes());
    pos += config.test_file.len();

    // Copy rest of the HTTP request
    let rest_template = " HTTP/1.1\r\nHost: ";
    request_buffer[pos..pos + rest_template.len()].copy_from_slice(rest_template.as_bytes());
    pos += rest_template.len();

    request_buffer[pos..pos + config.server_host.len()]
        .copy_from_slice(config.server_host.as_bytes());
    pos += config.server_host.len();

    let rest = "\r\nUser-Agent: Rust-WINC-SpeedTest/1.0\r\nConnection: close\r\n\r\n";
    request_buffer[pos..pos + rest.len()].copy_from_slice(rest.as_bytes());
    pos += rest.len();

    let http_request = &request_buffer[..pos];

    debug!(
        "HTTP request: {}",
        core::str::from_utf8(http_request).unwrap_or("invalid utf8")
    );

    // Send HTTP request
    let sent =
        block!(stack.send(&mut s, http_request)).map_err(|_| SpeedTestError::NetworkError)?;

    info!("HTTP request sent ({} bytes)", sent);

    // Initialize timing and counters
    let mut total_bytes = 0u32;
    let mut response_started = false;
    let mut header_complete = false;
    let mut report_counter = 0u32;

    let mut buffer = [0u8; 1024]; // Fixed buffer size for no_std compatibility

    info!("Starting download...");

    // Receive loop
    loop {
        match stack.receive(&mut s, &mut buffer) {
            Ok(bytes_received) => {
                if bytes_received == 0 {
                    // Connection closed
                    break;
                }

                if !response_started {
                    response_started = true;
                    info!("Download started - first bytes received");
                }

                // Simple header detection
                if !header_complete {
                    let response_slice = &buffer[..bytes_received];
                    let mut headers = [httparse::EMPTY_HEADER; 16];
                    let mut response = httparse::Response::new(&mut headers);
                    match response.parse(response_slice) {
                        Ok(httparse::Status::Complete(size)) => {
                            info!(
                                "HTTP headers complete code {} ( size  {} )",
                                response.code.unwrap_or(0),
                                size
                            );
                            header_complete = true;
                        }
                        Ok(httparse::Status::Partial) => {
                            error!("HTTP response not complete");
                        }
                        Err(e) => {
                            error!("-----Error parsing response: {:?}-----", WrapError(e));
                        }
                    }
                    if !matches!(response.code, Some(200)) {
                        error!("HTTP response code: {:?}", response.code);
                    }
                }

                total_bytes += bytes_received as u32;
                report_counter += 1;

                // Periodic progress report
                if report_counter >= config.report_interval {
                    let kb_received = total_bytes / 1024;
                    let elapsed = get_elapsed_seconds();
                    info!(
                        "Progress: {} KB received in {} seconds",
                        kb_received, elapsed as u32
                    );
                    report_counter = 0;
                }
            }
            Err(embedded_nal::nb::Error::WouldBlock) => {
                // No data available, continue
                continue;
            }
            Err(embedded_nal::nb::Error::Other(_e)) => {
                // Check if this is a connection close (normal for HTTP)
                // We expect ConnAborted when server closes the connection after sending data
                info!("Receive ended - connection closed by server");
                // For HTTP with Connection: close, any error is typically end-of-stream
                // so we'll treat this as normal completion
                break;
            }
        }
    }

    let elapsed = get_elapsed_seconds();
    let kb_total = total_bytes / 1024;
    let mb_total = kb_total / 1024;
    let average_speed_kbps = if elapsed > 0.0 {
        kb_total as f32 / elapsed
    } else {
        0.0
    };

    info!("=== Download Complete ===");
    info!("Total bytes received: {}", total_bytes);
    info!("Total size: {} KB ({} MB)", kb_total, mb_total);
    info!("Time elapsed: {} seconds", elapsed as u32);
    info!("Average speed: {} KB/s", average_speed_kbps as u32);

    stack.close(s).map_err(|_| SpeedTestError::NetworkError)?;

    Ok(SpeedTestResult {
        total_bytes,
        elapsed_seconds: elapsed,
        average_speed_kbps,
    })
}
