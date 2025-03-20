# Development

## Winc-rs crate
- Can be compiled and unit tested on host without a board
  - Run unit tests with `cargo test`
  - Generate test coverage with `cargo llvm-cov --html`
  - See docs: https://crates.io/crates/cargo-llvm-cov

## Embedded-hal Demos
- Located under `demos/` directory
  - Can be compiled and run on host
  - Run `cargo run -- --help` from demos directory to get a list of arguments
  - The same demo modules are used in `feather` demos that use `embedded-nal`

## Feather Wifi Demos and Tests
- Located in `feather/` directory
  - Board notes:
    - Adafruit board has factory programmed [SAMD bootloader](https://learn.adafruit.com/how-to-program-samd-bootloaders/overview)
    - Demos are currently not compatible with it, SAMD bootloader support is on TODO list
    - Use a JTAG programmer to work with the board instead (J-link or other)
    - JTAG connection directions are in [bootloader updating instructions](https://learn.adafruit.com/how-to-program-samd-bootloaders?view=all#feather-m0-m4-wiring)
    - After/before erasing the default bootloader, make sure the NVM control fuse for the bootloader (BOOTPROT) is also set to '0x00'; otherwise, the flash will stay locked for new programs. See instructions on the [Adafruit blog](https://learn.adafruit.com/how-to-program-samd-bootloaders/programming-the-bootloader-with-atmel-studio#un-set-bootloader-protection-fuse-3017004).
  - Development setup:
    - Install [`probe-rs`](https://probe.rs/)
    - Verify JTAG connection with `probe-rs list`
    - Run examples with `cargo run --example blinky`
      - To run release version ( faster download, smaller binary ):
         - `cargo run --release --example blinky`
    - Configure defmt logging with DEFMT_LOG environment variable
      - Bash: `export DEFMT_LOG=debug`
      - Powershell: `$env:DEFMT_LOG="debug"`
    - `cargo run` command uses config in `.cargo/config.toml`
      - There's a hardcoded `probe-rs run --speed 1100` in there
      - That speed may need tweaking, depending on your JTAG speed.
      - Can also be set with `PROBE_RS_SPEED` env var
  - Most examples use environment variables to set up test parameters. Set them
    before building and running, these get compiled into the binary
      - To connect the module to access point:
      - `export TEST_SSID=mywifi`
      - `export TEST_PASSWORD=mywifipassword`
      - Look for `option_env!` in test code to see what other parameters to change
  - Build all examples with `cargo build --examples --all-features`
  - Complex demos that require more dependencies are feature gated
  - Optional features: `iperf3`, `telnet`, `oled`, and `async`


## Pre-commit Hooks
- Uses [`pre-commit`](https://pre-commit.com)
  - Install on your system
  - Run `pre-commit install` in your local checkout
  - Automatically runs formatting and tests on staged files
  - Same checks run on pull requests
