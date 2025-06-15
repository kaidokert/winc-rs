//! WiFi Speed Test - Download large file and measure throughput
//!
//! Downloads test files from kaidokert.com to measure WiFi performance
//! Equivalent to the Arduino WifiSpeedTest for comparison
//!

#![no_main]
#![no_std]

use bsp::shared::parse_ip_octets;
use core::net::Ipv4Addr;
use core::sync::atomic::{AtomicU32, Ordering};
use cortex_m::peripheral::SYST;
use feather as bsp;
use wincwifi::StackError;

// Global counter for SYSTICK overflows
static OVERFLOW_COUNT: AtomicU32 = AtomicU32::new(0);

#[cortex_m_rt::exception]
fn SysTick() {
    // Increment the overflow counter
    OVERFLOW_COUNT.store(
        OVERFLOW_COUNT.load(Ordering::Relaxed) + 1,
        Ordering::Relaxed,
    );
}

// Get elapsed time in seconds since start
fn seconds() -> u32 {
    let overflows = OVERFLOW_COUNT.load(Ordering::Relaxed);
    overflows / 1000 // 1000 overflows = 1 second (10ms * 1000 = 10s)
}

mod runner;
use runner::{connect_and_run, ClientType, ReturnClient};

// Test server configuration
const TEST_SERVER_IP: &str = "18.155.192.71"; // kaidokert.com IP (AWS)
const TEST_SERVER_PORT: u16 = 80;
const TEST_SERVER_HOST: &str = "kaidokert.com";

// Test file options
const TEST_FILE_1MB: &str = "/test-file-1mb.json"; // 0.93 MB
const _TEST_FILE_10MB: &str = "/test-file-10mb.json"; // 9.37 MB

// Use the smaller file by default for embedded testing
const TEST_FILE: &str = TEST_FILE_1MB;

const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";

#[cortex_m_rt::entry]
fn main() -> ! {
    if let Err(something) = connect_and_run(
        "WiFi Speed Test",
        ClientType::Tcp,
        |stack: ReturnClient, _: core::net::Ipv4Addr| -> Result<(), StackError> {
            if let ReturnClient::Tcp(stack) = stack {
                // Enable SYSTICK interrupt
                let systick = unsafe { &*SYST::ptr() };
                unsafe {
                    // Enable SYSTICK interrupt
                    systick.csr.modify(|r| r | 1 << 1); // Set TICKINT bit
                }

                let ip_values: [u8; 4] = parse_ip_octets(TEST_SERVER_IP);
                let ip = Ipv4Addr::new(ip_values[0], ip_values[1], ip_values[2], ip_values[3]);

                defmt::info!("=== Starting WiFi Speed Test ===");
                defmt::info!("Server: {} ({})", TEST_SERVER_HOST, TEST_SERVER_IP);
                defmt::info!("File: {}", TEST_FILE);

                speed_test(stack, ip, TEST_SERVER_PORT)?;

                defmt::info!("=== Speed Test Complete ===");
            }
            Ok(())
        },
    ) {
        defmt::error!("Speed test failed: {}", something)
    } else {
        defmt::info!("Speed test completed successfully")
    };

    loop {
        cortex_m::asm::wfi();
    }
}

fn speed_test<T, S>(stack: &mut T, addr: Ipv4Addr, port: u16) -> Result<(), T::Error>
where
    T: embedded_nal::TcpClientStack<TcpSocket = S> + ?Sized,
    T::Error: embedded_nal::TcpError + defmt::Format,
{
    use core::net::{IpAddr, SocketAddr};
    use embedded_nal::nb::block;

    let sock = stack.socket();
    if let Ok(mut s) = sock {
        defmt::info!(
            "Connecting to {}.{}.{}.{}:{}",
            addr.octets()[0],
            addr.octets()[1],
            addr.octets()[2],
            addr.octets()[3],
            port
        );
        let remote = SocketAddr::new(IpAddr::V4(addr), port);
        block!(stack.connect(&mut s, remote))?;
        defmt::info!("Connected to server");

        // Build HTTP GET request dynamically
        let mut request_buffer = [0u8; 256];
        let request_str = "GET ";
        let mut pos = 0;

        // Copy "GET "
        request_buffer[pos..pos + request_str.len()].copy_from_slice(request_str.as_bytes());
        pos += request_str.len();

        // Copy the test file path
        request_buffer[pos..pos + TEST_FILE.len()].copy_from_slice(TEST_FILE.as_bytes());
        pos += TEST_FILE.len();

        // Copy rest of the HTTP request
        let rest = " HTTP/1.1\r\nHost: kaidokert.com\r\nUser-Agent: Rust-WINC-SpeedTest/1.0\r\nConnection: close\r\n\r\n";
        request_buffer[pos..pos + rest.len()].copy_from_slice(rest.as_bytes());
        pos += rest.len();

        let http_request = &request_buffer[..pos];
        defmt::info!(
            "HTTP request: {}",
            core::str::from_utf8(http_request).unwrap_or("invalid utf8")
        );

        // Send HTTP request
        let request_bytes = http_request;
        let sent = block!(stack.send(&mut s, request_bytes))?;
        defmt::info!("HTTP request sent ({} bytes)", sent);

        // Initialize timing and counters
        let mut total_bytes = 0u32;
        let mut response_started = false;
        let mut header_complete = false;
        let _last_report = 0u32;
        let report_interval = 32; // Report every ~32 receive calls
        let mut report_counter = 0u32;

        // Use a reasonable buffer size for embedded
        let mut buffer = [0u8; 1024];

        defmt::info!("Starting download...");

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
                        defmt::info!("Download started - first bytes received");
                    }

                    // Simple header detection - look for \r\n\r\n
                    if !header_complete {
                        let response_slice = &buffer[..bytes_received];
                        if let Ok(response_str) = core::str::from_utf8(response_slice) {
                            defmt::info!("HTTP Response start: {}", response_str);
                            if response_str.contains("\r\n\r\n") {
                                header_complete = true;
                                defmt::info!("HTTP headers complete");
                                // Check for HTTP error status
                                if response_str.contains("HTTP/1.1 404") {
                                    defmt::error!("HTTP 404 - File not found!");
                                } else if response_str.contains("HTTP/1.1 200") {
                                    defmt::info!("HTTP 200 - OK");
                                } else {
                                    defmt::warn!(
                                        "HTTP response: {}",
                                        &response_str
                                            [..response_str.find("\r\n").unwrap_or(50).min(50)]
                                    );
                                }
                            }
                        }
                    }

                    total_bytes += bytes_received as u32;
                    report_counter += 1;

                    // Periodic progress report
                    if report_counter >= report_interval {
                        let kb_received = total_bytes / 1024;
                        let elapsed = seconds();
                        defmt::info!(
                            "Progress: {} KB received in {} seconds",
                            kb_received,
                            elapsed
                        );
                        report_counter = 0;
                    }
                }
                Err(nb::Error::WouldBlock) => {
                    // No data available, continue
                    continue;
                }
                Err(nb::Error::Other(e)) => {
                    // Check if this is a connection close (normal for HTTP)
                    // We expect ConnAborted when server closes the connection after sending data
                    defmt::info!("Receive ended with: {:?}", e);
                    // For HTTP with Connection: close, any error is typically end-of-stream
                    // so we'll treat this as normal completion
                    break;
                }
            }
        }

        defmt::info!("=== Download Complete ===");
        defmt::info!("Total bytes received: {}", total_bytes);

        let kb_total = total_bytes / 1024;
        let mb_total = kb_total / 1024;
        let elapsed = seconds();
        defmt::info!("Total size: {} KB ({} MB)", kb_total, mb_total);
        defmt::info!("Time elapsed: {} seconds", elapsed);
        defmt::info!("Average speed: {} KB/s", kb_total / elapsed.max(1));

        // Note: For accurate speed calculation, we'd need precise timing
        // This requires a timer implementation which varies by platform
        defmt::info!("Speed calculation requires timer implementation");
        defmt::info!("Compare this total with Arduino results for relative performance");

        stack.close(s)?;
    } else {
        defmt::error!("Socket creation failed");
        // Can't easily convert StackError to T::Error, so we'll panic for now
        panic!("Socket creation failed");
    }

    Ok(())
}
