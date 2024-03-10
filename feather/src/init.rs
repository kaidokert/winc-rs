use super::bsp;
use super::hal;

use bsp::pac;

use pac::{CorePeripherals, Peripherals};

use hal::clock::GenericClockController;
use hal::time::{Hertz, MegaHertz};
use bsp::pin_alias;
use bsp::periph_alias;

use super::pins::Pins;

use cortex_m_systick_countdown::{PollingSysTick, SysTickCalibration};

pub enum FailureSource {
    Periph,
    Core,
    Clock,
}

pub fn init() -> Result<(PollingSysTick, bsp::RedLed), FailureSource> {
    let mut peripherals = Peripherals::take().ok_or(FailureSource::Periph)?;
    let mut core = CorePeripherals::take().ok_or(FailureSource::Core)?;

    let mut clocks = GenericClockController::with_internal_32kosc(
        peripherals.GCLK,
        &mut peripherals.PM,
        &mut peripherals.SYSCTRL,
        &mut peripherals.NVMCTRL,
    );

    let gclk0 = clocks.gclk0();
    let pins = Pins::new(peripherals.PORT);
    let red_led: bsp::RedLed = pin_alias!(pins.red_led).into();

    let hertz: Hertz = gclk0.into();
    let mut del = PollingSysTick::new(core.SYST, &SysTickCalibration::from_clock_hz(hertz.raw()));

    let spi_sercom = periph_alias!(peripherals.spi_sercom);

    let freq = MegaHertz::from_raw(1);

    let spi = bsp::spi_master(
        &mut clocks,
        freq.convert(),
        //400_u32.khz(),
        spi_sercom,
        &mut peripherals.PM,
        pins.sclk,
        pins.mosi,
        pins.miso,
    );

    Ok((del, red_led))
}
