#![no_std]

// Re-export everything we need from the core BSP
pub use feather_m0 as bsp;
pub use bsp::hal;
pub use bsp::Pins;
pub use bsp::pac;
pub use bsp::periph_alias;

pub use bsp::spi_master;

pub mod pins {
    use super::hal;

    hal::bsp_pins!(
        // Same as base board
        PA17 {
            name: d13
            aliases: {
                PushPullOutput: RedLed
            }
        }
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

        // Feather specific pins
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
    );

    pub use pin_alias;
}

pub use pins::*;
