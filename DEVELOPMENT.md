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
- Located in the `feather/` directory
  - Board notes:
    - The Adafruit board has a factory-programmed [SAMD bootloader](https://learn.adafruit.com/how-to-program-samd-bootloaders/overview).
    - Erasing the bootloader is not required, as demos can be flashed either through the bootloader using `bossa/bossac` or via a JTAG programmer (J-Link or equivalent).
    - JTAG connection instructions are in the [bootloader updating guide](https://learn.adafruit.com/how-to-program-samd-bootloaders?view=all#feather-m0-m4-wiring).
    - In case you want to erase the default bootloader, before or after erasing it, make sure the NVM control fuse for the bootloader (BOOTPROT) is set to `0x00`; otherwise, the board’s flash will remain locked. See the instructions on the [Adafruit blog](https://learn.adafruit.com/how-to-program-samd-bootloaders/programming-the-bootloader-with-atmel-studio#un-set-bootloader-protection-fuse-3017004).
  - Development setup:
    - Install the thumb target: `rustup target add thumbv6m-none-eabi`
    - Install [`probe-rs`](https://probe.rs/)
    - Install [`cargo-binutils`](https://github.com/rust-embedded/cargo-binutils)
    - Install [`BOSSA v1.9`](https://github.com/shumatech/BOSSA) to flash the binary via the [SAMD bootloader](https://learn.adafruit.com/how-to-program-samd-bootloaders/overview).
    - **With JTAG/SWD**
      - Verify the JTAG connection with `probe-rs list`.
      - **Bootloader Erased:** Run examples with:
        - `cargo run --example blinky`
        - To run the release version (faster download, smaller binary):
          - `cargo run --release --example blinky`
      - **Bootloader Not Erased:** Run examples with the `bootloader-enabled` feature:
        - `cargo run --example blinky --features="bootloader-enabled"`
        - To run the release version:
          - `cargo run --release --example blinky --features="bootloader-enabled"`
      - The `cargo run` command uses the configuration in `.cargo/config.toml`.
      - There is a hardcoded `probe-rs run --speed 1100` in the config.
      - That speed may need tweaking, depending on your JTAG speed.
      - It can also be set with the `PROBE_RS_SPEED` environment variable.
    - **With Bossa/Bossac**
      - Build the example with the `bootloader-enabled` feature flag and generate its binary file. Both steps can be done using the `cargo objcopy` command:
        - Debug version:
          `cargo objcopy --example blinky --features="bootloader-enabled" --no-default-features -- -O binary blinky.bin`
        - Release version:
          `cargo objcopy --release --example blinky --features="bootloader-enabled" --no-default-features -- -O binary blinky.bin`
      - Flashing:
        - Enter [bootloader mode](https://learn.adafruit.com/adafruit-feather-m4-express-atsamd51/uf2-bootloader-details#entering-bootloader-mode-2929745).
        - Use Bossac/Bossa to flash the binary:
          `bossac --port={port} -e -w -v -R --offset=0x2000 blinky.bin`
          - Replace `{port}` with the serial port your device is connected to. For example:
            - Linux: `/dev/ttyACM0`
            - Mac: `/dev/cu.usbmodem14301`
            - Windows: `COM32`
        - For flashing the binary using `bossa` versions `v1.7` or `v1.8`, see the instructions on the [Adafruit blog](https://learn.adafruit.com/adafruit-feather-m4-express-atsamd51/uf2-bootloader-details#running-bossac-on-the-command-line-2929769).
      - For logging, see [USB Serial Logging](#USB-Serial-Logging-(Alternative-to-JTAG)).
    - Configure defmt logging with DEFMT_LOG environment variable
      - Bash: `export DEFMT_LOG=debug`
      - Powershell: `$env:DEFMT_LOG="debug"`
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
