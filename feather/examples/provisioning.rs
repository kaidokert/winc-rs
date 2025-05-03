//! Provisioning Mode
//!

#![no_main]
#![no_std]

use feather as bsp;
use feather::init::init;

use bsp::shared::SpiStream;
use feather::hal::ehal::digital::OutputPin;
use feather::shared::{create_countdowns, delay_fn};
use wincwifi::{AccessPoint, StackError, WincClient};

fn program() -> Result<(), StackError> {
    if let Ok(mut ini) = init() {
        defmt::println!("Hello, Winc PRNG");
        let red_led = &mut ini.red_led;

        let mut cnt = create_countdowns(&ini.delay_tick);
        let mut delay_ms = delay_fn(&mut cnt.0);

        let mut stack = WincClient::new(SpiStream::new(ini.cs, ini.spi));

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
        // Configure the access point with WPA/WPA2 security using the provided SSID and password.
        let access_point = AccessPoint::wpa("test_winc_rs", "test1234556");
        // Start the provising mode.
        stack.start_provisioning_mode(access_point, "winctest", true)?;
        defmt::println!("Provisioning Started for 15 minutes");
        // Check for provisioning information is receieved for 15 minutes.
        let result = nb::block!(stack.get_provisioning_info(15));
        match result {
            Ok(info) => {
                defmt::info!("Credentials received from provisioning; connecting to access point.");
                // Connect to access point.
                nb::block!(stack.connect_to_ap(&info.ssid, &info.passphrase, false))?;
                defmt::info!("Connected to AP");
            }
            Err(err) => {
                if err == StackError::GeneralTimeout {
                    defmt::error!(
                        "No information was received for 15 minutes. Stopping provisioning mode."
                    );
                    stack.stop_provisioning_mode()?;
                } else {
                    defmt::error!("Provisioning Failed");
                }
            }
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
        defmt::error!("Error: {}", err);
        panic!("Error in main program");
    } else {
        defmt::info!("Good exit")
    };
    loop {}
}
