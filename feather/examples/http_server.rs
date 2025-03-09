//! Very hackish example http server to render a static index.html file
//! and control a LED on the board.

#![no_main]
#![no_std]

use bsp::shared::SpiStream;
use feather as bsp;
use feather::hal::ehal::digital::OutputPin;
use feather::init::init;
use feather::shared::{create_countdowns, delay_fn};

const DEFAULT_TEST_SSID: &str = "network";
const DEFAULT_TEST_PASSWORD: &str = "password";

use wincwifi::{StackError, WincClient};

use demos::http_server;

const HTTP_PORT: u16 = 80;

fn program() -> Result<(), StackError> {
    if let Ok(mut ini) = init() {
        defmt::println!("Hello, Winc Module");

        let mut cnt = create_countdowns(&ini.delay_tick);
        let red_led = &mut ini.red_led;

        let mut delay_ms = delay_fn(&mut cnt.0);
        let mut delay_ms2 = delay_fn(&mut cnt.1);

        let ssid = option_env!("TEST_SSID").unwrap_or(DEFAULT_TEST_SSID);
        let password = option_env!("TEST_PASSWORD").unwrap_or(DEFAULT_TEST_PASSWORD);
        defmt::info!(
            "Connecting to network: {} with password: {}",
            ssid,
            password
        );
        let mut stack = WincClient::new(SpiStream::new(ini.cs, ini.spi), &mut delay_ms2);

        let mut v = 0;
        loop {
            match stack.start_wifi_module() {
                Ok(_) => break,
                Err(nb::Error::WouldBlock) => {
                    defmt::debug!("Waiting start .. {}", v);
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

        defmt::info!("Started, connecting to AP ..");
        nb::block!(stack.connect_to_ap(ssid, password, false))?;

        defmt::debug!("Getting IP settings..");
        let info = nb::block!(stack.get_ip_settings())?;
        let ip = info.ip;

        defmt::info!(
            "Starting HTTP server at http://{}.{}.{}.{}:{}",
            ip.octets()[0],
            ip.octets()[1],
            ip.octets()[2],
            ip.octets()[3],
            HTTP_PORT
        );

        let mut send_index = |_body: &[u8], output: &mut [u8]| -> Result<usize, u16> {
            http_server::embed_index(output)
        };

        let mut led_state = false;
        let mut handle_led = |body: &[u8], output: &mut [u8]| -> Result<usize, u16> {
            if !body.is_empty() && body.contains(&b':') {
                led_state = body.windows(4).any(|w| w == b"true");
                if led_state {
                    red_led.set_high().map_err(|_| 500u16)?;
                } else {
                    red_led.set_low().map_err(|_| 500u16)?;
                }
            }
            let response = if led_state {
                b"{\"led\": true }"
            } else {
                b"{\"led\": false}"
            };
            output[..response.len()].copy_from_slice(response);
            Ok(response.len())
        };

        let mut paths = [
            http_server::Path {
                paths: http_server::INDEX_PATHS.as_slice(),
                handler: &mut send_index,
                is_json: false,
            },
            http_server::Path {
                paths: http_server::LED_PATHS.as_slice(),
                handler: &mut handle_led,
                is_json: true,
            },
        ];
        // This currently blocks forever
        http_server::http_server_args(&mut stack, HTTP_PORT, &mut paths)?;

        loop {
            stack.heartbeat().unwrap();
        }
    }
    Ok(())
}

#[cortex_m_rt::entry]
fn main() -> ! {
    if let Err(err) = program() {
        defmt::error!("Error: {}", err);
        panic!("Error in main program");
    } else {
        defmt::info!("Good exit")
    };
    loop {}
}
