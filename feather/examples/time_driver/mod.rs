// WARNING: sketch only, non-functional, not compiling

use core::sync::atomic::{AtomicU32, Ordering};
use cortex_m::peripheral::{syst, SYST};
use embassy_time_driver::Driver;

const SYSTICK_FREQ_HZ: u32 = 8_000_000; // Example: 8 MHz
const TICK_HZ: u64 = 1_000; // 1 ms resolution
const PERIOD: u32 = SYSTICK_FREQ_HZ / TICK_HZ as u32;

pub struct SystickDriver {
    systick: SYST,
    overflow_count: AtomicU32,
}

time_driver_impl!(static DRIVER: SystickDriver = SystickDriver {
    systick: unsafe { core::mem::zeroed() },
    overflow_count: AtomicU32::new(0),
});

impl SystickDriver {
    pub fn init(&'static self, mut systick: SYST) {
        systick.set_reload(PERIOD - 1);
        systick.clear_current();
        systick.set_clock_source(syst::SystClkSource::Core);
        systick.enable_counter();
        systick.enable_interrupt();
        unsafe {
            core::ptr::write(&self.systick as *const _ as *mut _, systick);
        }
    }
}

impl Driver for SystickDriver {
    fn now(&self) -> u64 {
        let overflow = self.overflow_count.load(Ordering::Relaxed);
        let current = PERIOD - 1 - self.systick.current();
        overflow * (PERIOD as u64) + (current as u64)
    }

    /*
    unsafe fn allocate_alarm(&self) -> Option<AlarmHandle> {
        None // No alarms available
    }

    fn set_alarm_callback(&self, _alarm: AlarmHandle, _callback: fn(*mut ()), _ctx: *mut ()) {
        // No-op: No alarms to set callbacks for
    }

    fn set_alarm(&self, _alarm: AlarmHandle, _timestamp: u64) -> bool {
        false // Always fail: No alarms supported
    }
    */
}

#[no_mangle]
extern "C" fn SysTick_Handler() {
    DRIVER.overflow_count.fetch_add(1, Ordering::Relaxed);
}

pub fn init(systick: SYST) {
    DRIVER.init(systick);
}
