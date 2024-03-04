#![no_main]
#![no_std]

use cortex_m_semihosting::debug;

use defmt_rtt as _; // global logger

// use some_hal as _; // memory layout
pub use feather_m0 as bsp;
pub use bsp::hal;

pub mod init;

use panic_probe as _;

// same panicking *behavior* as `panic-probe` but doesn't print a panic message
// this prevents the panic message being printed *twice* when `defmt::panic` is invoked
#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}

/// Terminates the application and makes a semihosting-capable debug tool exit
/// with status code 0.
pub fn exit() -> ! {
    loop {
        debug::exit(debug::EXIT_SUCCESS);
    }
}

/// Hardfault handler.
///
/// Terminates the application and makes a semihosting-capable debug tool exit
/// with an error. This seems better than the default, which is to spin in a
/// loop.
#[cortex_m_rt::exception]
unsafe fn HardFault(_frame: &cortex_m_rt::ExceptionFrame) -> ! {
    loop {
        debug::exit(debug::EXIT_FAILURE);
    }
}



pub mod pins {
    use super::hal;

    hal::bsp_pins!(
        PA17 {
            name: d13
            aliases: {
                PushPullOutput: RedLed
            }
        }
        PA18 {
            /// Pin 10, PWM capable
            name: d10
        }
        PA14 {
            name: winc_ena
            aliases: {
                PushPullOutput: WincEna
            }
        }
        PA08 {
            name: winc_rst
            aliases: {
                PushPullOutput: WincRst
            }
        }
        PA21 {
            name: winc_irq
            aliases: {
                PullUpInterrupt: WincIrq
            }
        }
        PA06 {
            name: winc_cs
            aliases: {
                PushPullOutput: WincCs
            }
        },
        PB11 {
            name: sclk
            aliases: {
                AlternateD: Sclk
            }
        }
        PB10 {
            name: mosi
            aliases: {
                AlternateD: Mosi
            }
        }
        PA12 {
            name: miso
            aliases: {
                AlternateD: Miso
            }
        }
    );
}
pub use pins::*;


// defmt-test 0.3.0 has the limitation that this `#[tests]` attribute can only be used
// once within a crate. the module can be in any file but there can only be at most
// one `#[tests]` module in this library crate
#[cfg(test)]
#[defmt_test::tests]
mod unit_tests {
    use defmt::assert;

    #[test]
    fn it_works() {
        assert!(true)
    }
}
