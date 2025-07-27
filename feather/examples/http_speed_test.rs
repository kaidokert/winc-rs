//! WiFi Speed Test - Download large file and measure throughput
//!
//! Downloads test files from kaidokert.com to measure WiFi performance
//! Equivalent to the Arduino WifiSpeedTest for comparison
//!

#![no_main]
#![no_std]

use bsp::shared::parse_ip_octets;
use core::net::Ipv4Addr;
use core::str::FromStr;
use core::sync::atomic::{AtomicU32, Ordering};
use cortex_m::peripheral::SYST;
use demos::http_speed_test::{
    speed_test, SpeedTestConfig, TEST_FILE_1MB, TEST_SERVER_HOST, TEST_SERVER_IP, TEST_SERVER_PORT,
};
use feather as bsp;
use feather::{error, info};
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

// Get elapsed time in seconds since start - with f32 interface for the new module
fn get_elapsed_seconds() -> f32 {
    let overflows = OVERFLOW_COUNT.load(Ordering::Relaxed);
    (overflows as f32) / 1000.0 // 1000 overflows = 1 second (10ms * 1000 = 10s)
}

mod runner;
use runner::{connect_and_run, ClientType, ReturnClient};

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
                let systick = unsafe { &*SYST::PTR };
                unsafe {
                    // Enable SYSTICK interrupt
                    systick.csr.modify(|r| r | 1 << 1); // Set TICKINT bit
                }

                let test_ip = option_env!("TEST_IP").unwrap_or(TEST_SERVER_IP);
                let ip_values: [u8; 4] = parse_ip_octets(test_ip)?;
                let ip = Ipv4Addr::new(ip_values[0], ip_values[1], ip_values[2], ip_values[3]);
                let test_port = option_env!("TEST_PORT").unwrap_or("");
                let port = u16::from_str(test_port).unwrap_or(TEST_SERVER_PORT);
                let test_host = option_env!("TEST_HOST").unwrap_or(TEST_SERVER_HOST);
                let test_file = option_env!("TEST_FILE").unwrap_or(TEST_FILE_1MB);

                info!("=== Starting WiFi Speed Test ===");
                info!("Server: {} ({})", test_host, test_ip);
                info!("File: {}", test_file);

                let config = SpeedTestConfig {
                    server_host: test_host,
                    test_file: test_file,
                    report_interval: 32,
                };

                match speed_test(stack, ip, port, config, get_elapsed_seconds) {
                    // Detailed results are printed in the test
                    Ok(_result) => {
                        info!("=== Speed Test Complete ===");
                    }
                    Err(e) => {
                        error!("Speed test failed: {:?}", e);
                        return Err(StackError::Unexpected);
                    }
                }
            }
            Ok(())
        },
    ) {
        error!("Speed test failed: {}", something);
    } else {
        info!("Speed test completed successfully")
    };

    loop {
        cortex_m::asm::wfi();
    }
}
