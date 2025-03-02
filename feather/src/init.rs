use super::bsp;
use super::hal;

use bsp::pac;

use pac::{CorePeripherals, Peripherals};

use bsp::periph_alias;
use bsp::pin_alias;
use core::convert::Infallible;
use hal::clock::GenericClockController;
use hal::time::{Hertz, MegaHertz};

use hal::prelude::*;

use super::shared::SpiBus;

// Set SPI bus to 4 Mhz, about as fast as it goes
const SPI_MHZ: u32 = 4;

use cortex_m_systick_countdown::{PollingSysTick, SysTickCalibration};

pub enum FailureSource {
    Periph,
    Core,
    Clock,
}

impl From<Infallible> for FailureSource {
    fn from(_: Infallible) -> Self {
        todo!()
    }
}

pub fn init() -> Result<
    (
        PollingSysTick,
        bsp::RedLed,
        impl hal::gpio::AnyPin,
        impl SpiBus,
    ),
    FailureSource,
> {
    let mut peripherals = Peripherals::take().ok_or(FailureSource::Periph)?;
    let core = CorePeripherals::take().ok_or(FailureSource::Core)?;

    let mut clocks = GenericClockController::with_internal_32kosc(
        peripherals.gclk,
        &mut peripherals.pm,
        &mut peripherals.sysctrl,
        &mut peripherals.nvmctrl,
    );

    let gclk0 = clocks.gclk0();
    let pins = bsp::pins::Pins::new(peripherals.port);
    let red_led: bsp::RedLed = bsp::pin_alias!(pins.red_led).into();

    let hertz: Hertz = gclk0.into();
    let mut del = PollingSysTick::new(core.SYST, &SysTickCalibration::from_clock_hz(hertz.raw()));

    let freq = MegaHertz::from_raw(SPI_MHZ);
    let spi_sercom = periph_alias!(peripherals.spi_sercom);
    let spi = bsp::spi_master(
        &mut clocks,
        freq.convert(),
        spi_sercom,
        &mut peripherals.pm,
        pins.sclk,
        pins.mosi,
        pins.miso,
    );

    let mut ena: bsp::WincEna = pin_alias!(pins.winc_ena).into(); // ENA
    let mut rst: bsp::WincRst = pin_alias!(pins.winc_rst).into(); // RST
    let mut cs: bsp::WincCs = pin_alias!(pins.winc_cs).into(); // CS

    ena.set_high()?; // ENable pin for the WiFi module, by default pulled down low, set HIGH to enable WiFi
    cs.set_high()?; // CS: pull low for transaction, high to end
    rst.set_high()?; // Reset pin for the WiFi module, controlled by the library

    del.delay_ms(500);

    cs.set_low()?; // CS: pull low for transaction, high to end
    rst.set_low()?;
    del.delay_ms(50);
    rst.set_high()?;
    cs.set_high()?; // CS: pull low for transaction, high to end

    del.delay_ms(500);

    Ok((del, red_led, cs, spi))
}
