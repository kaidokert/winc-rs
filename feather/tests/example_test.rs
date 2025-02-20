#![no_std]
#![no_main]

fn setup_log() {}

use defmt_rtt as _; // global logger

#[cfg(test)]
#[embedded_test::tests(setup=crate::setup_log())]
mod tests {
    use bsp::pac::Peripherals;
    pub use feather_m0 as bsp;

    #[init]
    fn init() -> Peripherals {
        Peripherals::take().unwrap()
    }

    #[test]
    fn minimal_test() {
        assert!(true);
    }

    #[test]
    fn minimal_test2() {
        assert!(false);
    }

    #[test]
    fn minimal_test3() {
        assert!(true);
    }
}
