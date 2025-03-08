#![no_std]
#![no_main]

use futures::join;

use cortex_m::Peripherals;

use embassy_time::Timer;
use systick_timer::SystickDriver;

use cortex_m_semihosting::debug::{self, EXIT_SUCCESS};
use cortex_m_semihosting::hprintln;

embassy_time_driver::time_driver_impl!(static DRIVER: SystickDriver<4>
    = SystickDriver::new(8_000_000, 7999));

#[cortex_m_rt::exception]
fn SysTick() {
    DRIVER.systick_interrupt();
}

async fn my_first_async_function() {
    Timer::after_micros(3).await;
}

async fn my_second_async_function() {
    Timer::after_micros(5).await;
}

#[embassy_executor::main]
async fn main(_s: embassy_executor::Spawner) {
    hprintln!("Initializing ..");
    let mut periph = Peripherals::take().unwrap();
    DRIVER.start(&mut periph.SYST);

    hprintln!("... started ...");
    Timer::after_micros(20).await;
    hprintln!("Done first wait");
    Timer::after_micros(10).await;
    hprintln!("Done second wait");

    let future1 = my_first_async_function();
    let future2 = my_second_async_function();
    let result = join!(future1, future2);
    hprintln!("Join completed");
    debug::exit(EXIT_SUCCESS);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
