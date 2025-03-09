use super::bsp;
use super::hal;

use bsp::pac;

use pac::{CorePeripherals, Peripherals};

use bsp::periph_alias;
use bsp::pin_alias;
use core::convert::Infallible;
use hal::clock::GenericClockController;
use hal::time::{Hertz, KiloHertz, MegaHertz};

use hal::ehal::digital::InputPin;
use hal::ehal::digital::OutputPin;
use hal::ehal::i2c::I2c;

use super::shared::SpiBus;

use systick_timer::SystickDriver;

// Set SPI bus to 4 Mhz, about as fast as it goes
const SPI_MHZ: u32 = 4;
const I2C_KHZ: u32 = 400;

const SYSTICK_FREQ: u64 = 48_000_000;
const SYSTICK_RELOAD: u32 = 79_999;

const MAX_TASKS: usize = 4;

embassy_time_driver::time_driver_impl!(static DRIVER: SystickDriver<MAX_TASKS>
    = SystickDriver::new(SYSTICK_FREQ, SYSTICK_RELOAD));

// Chip reset sequence timing. TODO: Shorten those as much as
// we reliably can
const WIFI_RESET_DELAY_DOWN: u64 = 50;
const WIFI_RESET_DELAY_UP: u64 = 20;
const WIFI_RESET_DELAY_WAIT: u64 = 50;

#[derive(Debug, defmt::Format)]
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

pub struct InitResult<
    SPI: SpiBus,
    I2C: I2c,
    OUTPUT1: OutputPin,
    OUTPUT2: hal::gpio::AnyPin, // todo: change this to OutputPin
    INPUT1: InputPin,
    INPUT2: InputPin,
    INPUT3: InputPin,
> {
    pub red_led: OUTPUT1,
    pub cs: OUTPUT2,
    pub spi: SPI,
    pub i2c: I2C,
    pub button_a: INPUT1,
    pub button_b: INPUT2,
    pub button_c: INPUT3,
}

pub async fn init() -> Result<
    InitResult<
        impl SpiBus,
        impl I2c,
        impl OutputPin,
        impl hal::gpio::AnyPin,
        impl InputPin,
        impl InputPin,
        impl InputPin,
    >,
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

    let hertz: Hertz = clocks.gclk0().into();
    assert_eq!(hertz.raw() as u64, SYSTICK_FREQ);
    let calibrated_tick_value = cortex_m::peripheral::SYST::get_ticks_per_10ms();
    assert_eq!(calibrated_tick_value, SYSTICK_RELOAD);

    let mut syst = core.SYST;
    DRIVER.start(&mut syst);

    let pins = bsp::pins::Pins::new(peripherals.port);
    let red_led: bsp::RedLed = bsp::pin_alias!(pins.red_led).into();

    let i2c = bsp::i2c_master(
        &mut clocks,
        KiloHertz::from_raw(I2C_KHZ).convert(),
        periph_alias!(peripherals.i2c_sercom),
        &mut peripherals.pm,
        pins.sda,
        pins.scl,
    );
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

    OutputPin::set_high(&mut ena)?; // Enable pin for the WiFi module, by default pulled down low, set HIGH to enable WiFi
    OutputPin::set_high(&mut cs)?; // CS: pull low for transaction, high to end
    OutputPin::set_high(&mut rst)?; // Reset pin for the WiFi module, controlled by the library

    embassy_time::Timer::after_millis(WIFI_RESET_DELAY_DOWN).await;
    OutputPin::set_low(&mut cs)?; // CS: pull low for transaction, high to end
    OutputPin::set_low(&mut rst)?;
    embassy_time::Timer::after_millis(WIFI_RESET_DELAY_UP).await;
    OutputPin::set_high(&mut rst)?;
    OutputPin::set_high(&mut cs)?; // CS: pull low for transaction, high to end
    embassy_time::Timer::after_millis(WIFI_RESET_DELAY_WAIT).await;

    Ok(InitResult {
        red_led,
        cs,
        spi,
        i2c,
        button_a: pins.d9.into_pull_up_input(),
        button_b: pins.d6.into_pull_up_input(),
        button_c: pins.d5.into_pull_up_input(),
    })
}

#[cortex_m_rt::exception]
fn SysTick() {
    DRIVER.systick_interrupt();
}
