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
    - Demos are currently not compatible with it, SAMD bootloader support is on TODO list #20
    - Use a JTAG programmer to work with the board instead (J-link or other)
    - JTAG connection directions are in [bootloader updating instructions](https://learn.adafruit.com/how-to-program-samd-bootloaders?view=all#feather-m0-m4-wiring)
    - After/before erasing the default bootloader, make sure the NVM control fuse for the bootloader (BOOTPROT) is also set to '0x00'; otherwise, the board's flash will stay locked. See instructions on the [Adafruit blog](https://learn.adafruit.com/how-to-program-samd-bootloaders/programming-the-bootloader-with-atmel-studio#un-set-bootloader-protection-fuse-3017004).
  - Development setup:
    - Install thumb target: `rustup target add thumbv6m-none-eabi`
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

## USB Serial Logging (Alternative to JTAG)
- USB serial and log output can be enabled with `log` and `usb` features
- This enables Arduino-style development without requiring JTAG/probe-rs
- The default logging output is sent to `defmt`, and it's mutually exclusive with `log`
- Features:
  - `usb`: Enables USB CDC serial device functionality
  - `log`: Enables log crate integration with USB output

### Usage
- To run an example with USB output:
    ```bash
    cargo run --no-default-features --features="usb,log" --example http_speed_test
    ```
- Connect to USB serial port (usually `/dev/ttyACM0` on Linux, `COMx` on Windows)
- Use any serial terminal: `screen /dev/ttyACM0`, Arduino IDE Serial Monitor, etc.

### Log Level Control
- USB logging level is controlled by `FEATHER_USB_LOG` environment variable (compile-time)
- Supports standard logging levels: `error`, `warn`, `info` (default), `debug`, `trace`
- Example: `FEATHER_USB_LOG=warn cargo run --no-default-features --features="usb,log" --example myapp`

### Comparison with defmt
- **defmt** (default): Uses JTAG/RTT for logging, requires probe-rs
- **USB serial logging**: Uses USB serial, slower but works without JTAG hardware

## Pre-commit Hooks
- Uses [`pre-commit`](https://pre-commit.com)
  - Install on your system
  - Run `pre-commit install` in your local checkout
  - Automatically runs formatting and tests on staged files
  - Same checks run on pull requests

## Environment Variables Reference
Common variables used in examples (set before building):

### WiFi Connection (Most Examples)
- `TEST_SSID=mywifi` - WiFi network name
- `TEST_PASSWORD=mywifipassword` - WiFi password

### Logging Control
- `DEFMT_LOG=debug` - defmt logging level (trace/debug/info/warn/error)
- `FEATHER_USB_LOG=info` - USB serial logging level (trace/debug/info/warn/error)

### Network Testing
- `TEST_IP=192.168.1.1` - Target IP for ping/connection tests
- `TEST_PORT=8080` - Target port for TCP/UDP tests
- `TEST_HOST=httpbin.org` - Target hostname for HTTP/DNS tests
- `TEST_TTL=200` - Ping TTL value
- `TEST_COUNT=4` - Number of ping attempts

### Specialized Tests
- `TEST_AP_SSID=provision_ssid` - Access point SSID for provisioning
- `TEST_AP_PASSWORD=provision_pass` - Access point password for provisioning
- `TEST_AP_DNS=provision.local` - DNS hostname for provisioning
- `TEST_FILE=/test-file-1mb.json` - HTTP download test file path
- `TEST_IPERF_IP=192.168.1.100` - iPerf3 server IP
- `TEST_IPERF_PORT=5201` - iPerf3 server port
- `TEST_IPERF_UDP=false` - Use UDP for iPerf3 (true/false)
- `NUM_BYTES=1048576` - Number of bytes for iPerf3 test
- `BLOCK_LEN=1024` - Block size for iPerf3 test
- `LOOP_FOREVER=false` - Run server examples indefinitely (true/false)

### Development
- `PROBE_RS_SPEED=1100` - JTAG probe speed

See `option_env!` calls in example source code for complete parameter lists.
