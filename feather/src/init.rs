use super::bsp;
use super::hal;

use bsp::pac;

use hal::time::KiloHertz;
use pac::{CorePeripherals, Peripherals};

use bsp::periph_alias;
use bsp::pin_alias;
use core::convert::Infallible;
#[cfg(feature = "irq")]
use cortex_m::peripheral::NVIC;
use hal::clock::GenericClockController;
#[cfg(feature = "irq")]
use hal::eic::Eic;
#[cfg(feature = "irq")]
use hal::eic::*;
#[cfg(all(feature = "irq", not(feature = "irq-override")))]
use pac::interrupt;

use hal::time::{Hertz, MegaHertz};

use hal::prelude::*;

use hal::ehal::digital::InputPin;
use hal::ehal::digital::OutputPin;
use hal::ehal::i2c::I2c;

use super::shared::SpiBus;

// Set SPI bus to 4 Mhz, about as fast as it goes
const SPI_MHZ: u32 = 4;
const I2C_KHZ: u32 = 400;

use cortex_m_systick_countdown::{PollingSysTick, SysTickCalibration};

// Chip reset sequence timing. TODO: Shorten those as much as
// we reliably can
const WIFI_RESET_DELAY_DOWN: u32 = 50;
const WIFI_RESET_DELAY_UP: u32 = 20;
const WIFI_RESET_DELAY_WAIT: u32 = 50;

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
    OUTPUT2: OutputPin,
    INPUT1: InputPin,
    INPUT2: InputPin,
    INPUT3: InputPin,
> {
    pub delay_tick: PollingSysTick,
    pub red_led: OUTPUT1,
    pub cs: OUTPUT2,
    pub spi: SPI,
    pub i2c: I2C,
    pub button_a: INPUT1,
    pub button_b: INPUT2,
    pub button_c: INPUT3,
}

pub fn init() -> Result<
    InitResult<
        impl SpiBus,
        impl I2c,
        impl OutputPin,
        impl OutputPin,
        impl InputPin,
        impl InputPin,
        impl InputPin,
    >,
    FailureSource,
> {
    let mut peripherals = Peripherals::take().ok_or(FailureSource::Periph)?;
    #[cfg(not(feature = "irq"))]
    let core = CorePeripherals::take().ok_or(FailureSource::Core)?;
    #[cfg(feature = "irq")]
    let mut core = CorePeripherals::take().ok_or(FailureSource::Core)?;

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

    // Power Manager
    let mut pm = peripherals.pm;

    let i2c = bsp::i2c_master(
        &mut clocks,
        KiloHertz::from_raw(I2C_KHZ).convert(),
        periph_alias!(peripherals.i2c_sercom),
        &mut pm,
        pins.sda,
        pins.scl,
    );
    let freq = MegaHertz::from_raw(SPI_MHZ);
    let spi_sercom = periph_alias!(peripherals.spi_sercom);
    let spi = bsp::spi_master(
        &mut clocks,
        freq.convert(),
        spi_sercom,
        &mut pm,
        pins.sclk,
        pins.mosi,
        pins.miso,
    );

    let mut ena: bsp::WincEna = pin_alias!(pins.winc_ena).into(); // ENA
    let mut rst: bsp::WincRst = pin_alias!(pins.winc_rst).into(); // RST
    let mut cs: bsp::WincCs = pin_alias!(pins.winc_cs).into(); // CS
    #[cfg(feature = "irq")]
    {
        let irq: bsp::WincIrq = pin_alias!(pins.winc_irq).into(); // Get IRQ pin
        let eic_clock = clocks.eic(&gclk0).ok_or(FailureSource::Clock)?; // Enable clock for interrupt controller
        let eic = Eic::new(&mut pm, eic_clock, peripherals.eic); // Configure the interrupt controller
        let channels = eic.split(); // Get Channels of EIC
        let mut extint = irq.into_pull_up_ei(channels.5); // Set Channel 5 to EXINT
        extint.sense(hal::eic::Sense::Fall);
        extint.enable_interrupt();
        // Enable EIC interrupt in the NVIC
        unsafe {
            core.NVIC.set_priority(pac::interrupt::EIC, 1);
            NVIC::unmask(pac::interrupt::EIC);
        }

        let mut button_c = pins.d5.into_pull_up_ei(channels.15);
        button_c.sense(hal::eic::Sense::Fall);
        button_c.enable_interrupt();
    }

    OutputPin::set_high(&mut ena)?; // Enable pin for the WiFi module, by default pulled down low, set HIGH to enable WiFi
    OutputPin::set_high(&mut cs)?; // CS: pull low for transaction, high to end
    OutputPin::set_high(&mut rst)?; // Reset pin for the WiFi module, controlled by the library

    del.delay_ms(WIFI_RESET_DELAY_DOWN);
    OutputPin::set_low(&mut cs)?; // CS: pull low for transaction, high to end
    OutputPin::set_low(&mut rst)?;
    del.delay_ms(WIFI_RESET_DELAY_UP);
    OutputPin::set_high(&mut rst)?;
    OutputPin::set_high(&mut cs)?; // CS: pull low for transaction, high to end
    del.delay_ms(WIFI_RESET_DELAY_WAIT);

    Ok(InitResult {
        delay_tick: del,
        red_led,
        cs,
        spi,
        i2c,
        button_a: pins.d9.into_pull_up_input(),
        button_b: pins.d6.into_pull_up_input(),
        button_c: pins.d0.into_pull_up_input(),
    })
}

#[cfg(all(feature = "irq", not(feature = "irq-override")))]
#[interrupt]
fn EIC() {
    unsafe {
        // Accessing registers from interrupts context is safe
        let eic = &*pac::Eic::ptr();

        let flag5 = eic.intflag().read().extint5().bit_is_set();
        if flag5 {
            eic.intflag().modify(|_, w| w.extint5().set_bit());
        }
    }
}
