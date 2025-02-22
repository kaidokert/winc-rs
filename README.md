# WINC1500 Rust

[![crate](https://img.shields.io/crates/v/wincwifi.svg)](https://crates.io/crates/wincwifi)
[![documentation](https://docs.rs/wincwifi/badge.svg)](https://docs.rs/wincwifi/)
[![Build](https://github.com/kaidokert/winc-rs/actions/workflows/rust.yaml/badge.svg)](https://github.com/kaidokert/winc-rs/actions/workflows/rust.yaml)
[![Coverage Status](https://coveralls.io/repos/github/kaidokert/winc-rs/badge.svg?branch=main)](https://coveralls.io/github/kaidokert/winc-rs?branch=main)

Code to interface with ATWINC1500 Wifi chip from Rust.
Tested on [Adafruit Feather M0 WiFi](https://www.adafruit.com/product/3010).

[winc-rs](winc-rs) is the crate that implements chip access, see its [README](winc-rs/README.md) for more info.

[feather](feather) dir has examples running on the Feather board.

[demos](demos) are the & testing programs that Feather examples use, written with
[embedded-nal](https://github.com/rust-embedded/embedded-nal).
These are also separately runnable with `std-embedded-nal` crate.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for details.

## License

Apache 2.0; see [`LICENSE`](LICENSE) for details.

## Disclaimer

This project is not an official Google project. It is not supported by
Google and Google specifically disclaims all warranties as to its quality,
merchantability, or fitness for a particular purpose.
