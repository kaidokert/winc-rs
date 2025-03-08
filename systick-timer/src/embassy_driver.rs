use core::cell::RefCell;
use cortex_m::{
    interrupt::{self, Mutex},
    peripheral::SYST,
};
use cortex_m_rt::exception;

struct Wakeup {
    wakeup_at: u64,
    waker: core::task::Waker,
}

/// Very basic Embassy time driver that uses the SysTick timer.
///
/// Wakeups are stored in a fixed-size array
///
/// The driver has to be a static instance, create it with:
///
/// ```
/// embassy_time_driver::time_driver_impl!(static DRIVER: SystickDriver<4>
///     = SystickDriver::new(48_000_000, 47999));
/// ```
///
pub struct SystickDriver<const N: usize> {
    wakeup_at: Mutex<RefCell<[Option<Wakeup>; N]>>,
    timer: crate::Timer,
}

impl<const N: usize> SystickDriver<N> {
    /// SystickDriver constructor.
    ///
    /// # Arguments
    ///
    /// * `systick_freq` - The frequency of the SysTick timer in Hz.
    /// * `reload_value` - The reload value for the SysTick timer.
    ///
    ///  Note the tick frequency is configured to embassy_time_driver::TICK_HZ.
    ///
    pub const fn new(systick_freq: u64, reload_value: u32) -> Self {
        let timer = cortex::Timer::new(embassy_time_driver::TICK_HZ, reload_value, systick_freq);
        Self {
            wakeup_at: Mutex::new(RefCell::new([const { None }; N])),
            timer: timer,
        }
    }
    fn maybe_wake(&self) {
        interrupt::free(|cs| {
            let mutex_borrow = &self.wakeup_at.borrow(cs);
            for slot in mutex_borrow.borrow_mut().iter_mut() {
                let mut cleared = false;
                if let Some(wakeup) = slot {
                    if self.timer.now() >= wakeup.wakeup_at {
                        wakeup.waker.wake_by_ref();
                        cleared = true;
                    }
                }
                if cleared {
                    *slot = None;
                }
            }
        })
    }

    /// Call this from the SysTick interrupt handler.
    pub fn systick_interrupt(&self) {
        self.timer.systick_handler();
        self.maybe_wake();
    }
}

impl<const N: usize> embassy_time_driver::Driver for SystickDriver<N> {
    fn now(&self) -> u64 {
        DRIVER.timer.now()
    }

    fn schedule_wake(&self, at: u64, waker: &core::task::Waker) {
        interrupt::free(|cs| {
            let mutex_borrow = self.wakeup_at.borrow(cs);
            let mut found = false;
            for slot in mutex_borrow.borrow_mut().iter_mut() {
                if slot.is_none() {
                    *slot = Some(Wakeup {
                        wakeup_at: at,
                        waker: waker.clone(),
                    });
                    found = true;
                    break;
                }
            }
            if !found {
                panic!("No free wakeup slots");
            }
        })
    }
}

pub fn init(syst: &mut SYST) {
    DRIVER.timer.start(syst);
}

#[cfg(feature = "embassy-defaults")]
embassy_time_driver::time_driver_impl!(static DRIVER: SystickDriver<4> = SystickDriver::new(48_000_000, 47999));

#[cfg(feature = "embassy-defaults")]
#[exception]
fn SysTick() {
    DRIVER.systick_interrupt();
}
