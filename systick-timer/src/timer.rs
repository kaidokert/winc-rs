use core::sync::atomic::{AtomicU32, Ordering};

#[cfg(feature = "cortex-m")]
use cortex_m::peripheral::{syst::SystClkSource, SYST};

/// A 64-bit timer based on SysTick.
///
/// Stores wraparounds in 2 32-bit atomics. Scales the systick counts
/// to arbitrary frequency.
pub struct Timer {
    inner_wraps: AtomicU32, // Counts SysTick interrupts (lower 32 bits)
    outer_wraps: AtomicU32, // Counts overflows of inner_wraps (upper 32 bits)
    reload_value: u32,      // SysTick reload value (max 2^24 - 1)
    multiplier: u64,        // Precomputed for scaling cycles to ticks
    shift: u32,             // Precomputed for scaling efficiency
    #[cfg(test)]
    current_systick: u32,
}

impl Timer {
    /// SysTick handler.
    ///
    /// Call this from the SysTick interrupt handler.
    pub fn systick_handler(&self) {
        // Increment inner_wraps and check for overflow
        let inner = self.inner_wraps.load(Ordering::Relaxed);
        // Check for overflow (inner was u32::MAX)
        // Store the incremented value
        self.inner_wraps
            .store(inner.wrapping_add(1), Ordering::SeqCst);
        if inner == u32::MAX {
            // Increment outer_wraps
            let outer = self.outer_wraps.load(Ordering::Relaxed).wrapping_add(1);
            self.outer_wraps.store(outer, Ordering::SeqCst);
        }
    }

    /// Interrupt handler for nested interrupts.
    ///
    /// Call this instead of systick_handler from the interrupt handler, if
    /// you have nested interrupts enabled.
    #[cfg(feature = "cortex-m")]
    pub fn systick_interrupt_for_nested(&self) {
        cortex_m::interrupt::free(|_| {
            self.systick_handler();
        })
    }

    /// Returns the current 64-bit tick count, scaled to the configured frequency `tick_hz`.
    pub fn now(&self) -> u64 {
        // Note: This does not enter critical section and block other interrupts.
        loop {
            // Load current counter values
            let inner1 = self.inner_wraps.load(Ordering::SeqCst) as u64;
            let outer = self.outer_wraps.load(Ordering::SeqCst) as u64;
            let inner2 = self.inner_wraps.load(Ordering::SeqCst) as u64;
            // Detect if interrupt happened between the two loads
            if inner1 == inner2 {
                let current = self.get_syst() as u64; // Current SysTick counter value

                // Total cycles = (total interrupts * cycles per interrupt) + remaining cycles
                let reload = self.reload_value as u64;
                let total_interrupts = (outer << 32) | inner1;
                let total_cycles = total_interrupts * (reload + 1) + (reload - current);

                // Scale to ticks (e.g., microseconds) using precomputed multiplier and shift
                return (total_cycles * self.multiplier) >> self.shift;
            }
        }
    }

    /// Returns the current SysTick counter value.
    const fn get_syst(&self) -> u32 {
        #[cfg(test)]
        return self.current_systick;

        #[cfg(all(not(test), feature = "cortex-m"))]
        return cortex_m::peripheral::SYST::get_current();

        #[cfg(all(not(test), not(feature = "cortex-m")))]
        panic!("This module requires the cortex-m crate to be available");
    }

    // Figure out a shift that leads to less precision loss
    const fn compute_shift(tick_hz: u64, systick_freq: u64) -> u32 {
        let mut shift = 32;
        let mut multiplier = (tick_hz << shift) / systick_freq;
        while multiplier == 0 && shift < 64 {
            shift += 1;
            multiplier = (tick_hz << shift) / systick_freq;
        }
        shift
    }

    /// Creates a new timer that converts SysTick cycles to ticks at a specified frequency.
    ///
    /// # Arguments
    ///
    /// * `tick_hz` - The desired output frequency in Hz (e.g., 1000 for millisecond ticks)
    /// * `reload_value` - The SysTick reload value. Must be between 1 and 2^24-1.
    ///                    This determines how many cycles occur between interrupts.
    /// * `systick_freq` - The frequency of the SysTick counter in Hz (typically CPU frequency)
    ///
    /// # Panics
    ///
    /// * If `reload_value` is 0 or greater than 2^24-1 (16,777,215)
    /// * If `systick_freq` is 0
    ///
    /// # Examples
    ///
    /// ```
    /// # use systick_timer::Timer;
    /// // Create a millisecond-resolution timer on a 48MHz CPU with reload value of 47,999
    /// let timer = Timer::new(1000, 47_999, 48_000_000);
    /// ```
    pub const fn new(tick_hz: u64, reload_value: u32, systick_freq: u64) -> Self {
        if reload_value > (1 << 24) - 1 {
            panic!("Reload value too large");
        }
        if reload_value == 0 {
            panic!("Reload value cannot be 0");
        }

        // Use a shift to maintain precision and keep multiplier within u64
        let shift = Self::compute_shift(tick_hz, systick_freq);
        let multiplier = (tick_hz << shift) / systick_freq;

        Timer {
            inner_wraps: AtomicU32::new(0),
            outer_wraps: AtomicU32::new(0),
            reload_value,
            multiplier,
            shift,
            #[cfg(test)]
            current_systick: 0,
        }
    }

    /// Call this if you haven't already started the timer.
    #[cfg(feature = "cortex-m")]
    pub fn start(&self, syst: &mut SYST) {
        syst.set_clock_source(cortex_m::peripheral::syst::SystClkSource::Core);
        syst.set_reload(self.reload_value);
        syst.clear_current();
        syst.enable_interrupt();
        syst.enable_counter();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn test_zero_systick_freq() {
        Timer::new(1000, 5, 0);
    }

    #[test]
    fn test_timer_new() {
        let mut timer = Timer::new(1000, 5, 12_000);
        timer.inner_wraps.store(4, Ordering::Relaxed); // 4 interrupts = 24 cycles
        timer.current_systick = 3; // Start of next period
        assert_eq!(timer.now(), 2); // Should be ~2 ticks
    }

    #[test]
    fn test_compute_shift() {
        assert_eq!(Timer::compute_shift(1000, 12_000), 32);
        // This ratio overflows 32bit, so we shift
        assert_eq!(Timer::compute_shift(3, 16_000_000_000), 33);
    }

    #[test]
    fn test_timer_initial_state() {
        let timer = Timer::new(1000, 5, 12_000);
        assert_eq!(timer.now(), 0);
    }

    struct TestTimer<const RELOAD: u32> {
        timer: Timer,
    }
    impl<const RELOAD: u32> TestTimer<RELOAD> {
        fn new(tick_hz: u64, systick_freq: u64) -> Self {
            Self {
                timer: Timer::new(tick_hz, RELOAD, systick_freq),
            }
        }
        fn interrupt(&mut self) {
            self.timer.systick_handler();
            self.timer.current_systick = RELOAD;
        }
        fn set_tick(&mut self, tick: u32) -> u64 {
            assert!(tick <= RELOAD);
            self.timer.current_systick = tick;
            self.timer.now()
        }
    }

    #[test]
    fn test_timer_matching_rates() {
        let mut timer = TestTimer::<5>::new(1000, 1000);
        assert_eq!(timer.set_tick(5), 0);
        assert_eq!(timer.set_tick(4), 1);
        assert_eq!(timer.set_tick(0), 5);
        timer.interrupt();
        assert_eq!(timer.set_tick(5), 6);
    }

    #[test]
    fn test_timer_tick_rate_2x() {
        let mut timer = TestTimer::<5>::new(2000, 1000);
        assert_eq!(timer.set_tick(5), 0);
        assert_eq!(timer.set_tick(4), 2);
        assert_eq!(timer.set_tick(0), 10);
        timer.interrupt();
        assert_eq!(timer.set_tick(5), 12);
        timer.interrupt();
        assert_eq!(timer.set_tick(5), 24);
    }

    #[test]
    fn test_systick_rate_2x() {
        let mut timer = TestTimer::<5>::new(1000, 2000);
        assert_eq!(timer.set_tick(5), 0);
        assert_eq!(timer.set_tick(4), 0);
        assert_eq!(timer.set_tick(3), 1);
        assert_eq!(timer.set_tick(2), 1);
        assert_eq!(timer.set_tick(0), 2);
        timer.interrupt();
        assert_eq!(timer.set_tick(5), 3);
        timer.interrupt();
        assert_eq!(timer.set_tick(5), 6);
    }

    #[test]
    fn test_outer_wraps_wrapping() {
        let mut timer = TestTimer::<5>::new(1000, 1000);
        // Set up for outer_wraps overflow
        timer.timer.inner_wraps.store(u32::MAX, Ordering::Relaxed);
        timer.timer.outer_wraps.store(u32::MAX, Ordering::Relaxed);
        timer.timer.current_systick = 5;

        // One more interrupt should wrap outer_wraps
        timer.interrupt();
        // Should still count correctly despite wrapping
        // With matching rates, we expect total_cycles * (1000/1000) ticks
        assert_eq!(timer.set_tick(5), ((1u128 << 64) * 1000 / 1000) as u64);
    }

    #[test]
    fn test_extreme_rates() {
        // Test with very high tick rate vs systick rate (1000:1)
        let mut timer = TestTimer::<5>::new(1_000_000, 1000);
        assert_eq!(timer.set_tick(5), 0);
        timer.interrupt(); // One interrupt = 6 cycles, each cycle = 1000 ticks
        assert_eq!(timer.set_tick(5), 6000); // 6 cycles * 1000 ticks/cycle

        // Test with very low tick rate vs systick rate (1:1000)
        let mut timer = TestTimer::<5>::new(1000, 1_000_000);
        // With 1000:1 ratio and reload of 5 (6 cycles per interrupt)
        // We need (1_000_000/1000 * 6) = 6000 cycles for 6 ticks
        // So we need 1000 interrupts for 6 ticks
        for _ in 0..1000 {
            timer.interrupt();
        }
        assert_eq!(timer.set_tick(5), 5); // Should get 5 complete ticks
    }

    #[test]
    fn test_boundary_conditions() {
        // Test with minimum reload value
        let mut timer = TestTimer::<1>::new(1000, 1000);
        assert_eq!(timer.set_tick(1), 0);
        assert_eq!(timer.set_tick(0), 1);
        timer.interrupt();
        assert_eq!(timer.set_tick(1), 2);

        // Test with maximum reload value
        let mut timer = TestTimer::<0xFFFFFF>::new(1000, 1000);
        assert_eq!(timer.set_tick(0xFFFFFF), 0);
        assert_eq!(timer.set_tick(0xFFFF00), 255);
        assert_eq!(timer.set_tick(0), 0xFFFFFF);
    }

    #[test]
    fn test_partial_tick_accuracy() {
        // With matching rates, test partial periods
        let mut timer = TestTimer::<100>::new(1000, 1000);
        assert_eq!(timer.set_tick(100), 0); // Start of period
        assert_eq!(timer.set_tick(75), 25); // 25% through period = 25 ticks
        assert_eq!(timer.set_tick(50), 50); // 50% through period = 50 ticks
        assert_eq!(timer.set_tick(25), 75); // 75% through period = 75 ticks
        assert_eq!(timer.set_tick(0), 100); // End of period = 100 ticks
    }

    #[test]
    fn test_interrupt_race() {
        let mut timer = TestTimer::<5>::new(1000, 1000);
        timer.interrupt();
        timer.timer.current_systick = 3;
        let t1 = timer.timer.now();
        timer.interrupt();
        let t2 = timer.timer.now();
        assert!(t2 > t1); // Monotonicity
    }

    #[test]
    fn test_rapid_interrupts() {
        let mut timer = TestTimer::<5>::new(1000, 1000);
        // With matching rates, each interrupt = 6 cycles = 6 ticks
        for _ in 0..10 {
            timer.interrupt();
        }
        // 10 interrupts * 6 cycles/interrupt * (1000/1000) = 60 ticks
        assert_eq!(timer.set_tick(5), 60);

        // At position 2, we're 3 cycles in = 3 more ticks
        assert_eq!(timer.set_tick(2), 63);
    }
}
