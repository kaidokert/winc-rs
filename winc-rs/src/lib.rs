// Copyright 2023 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Dual logging system compatibility: defmt doesn't support modern format syntax
#![allow(clippy::uninlined_format_args)]

//! ATWINC1500 Wifi module library
//!
//! NOTE: This very much Work In Progress
//!
//! The main entry point is [WincClient].
//!
//! The following traits are implemented:
//!
//! - [embedded_nal::TcpClientStack] and [embedded_nal::TcpFullStack]
//! - [embedded_nal::UdpClientStack] and [embedded_nal::UdpFullStack]
//! - [embedded_nal::Dns]
//!
//! [Examples are available](https://github.com/kaidokert/winc-rs/tree/main/feather/examples)
//! for [Adafruit Feather M0 WiFi](https://www.adafruit.com/product/3010) board.
//!
//! The low-lever library is in the internal `manager` module, it's the part that
//! wraps the HIF protocol and the chip registers.
//!
//! Connecting to AP, getting an IP, DNS lookups etc are implemented.
//!
//! Basic usage:
//! ```no_run
//! # use wincwifi::{WincClient, WifiChannel, Ssid, Credentials, WpaKey};
//! # use embedded_nal::{nb, AddrType, Dns};
//! # fn del_fn(ms: u32) {}
//! # let mut buffer = [0; 1];
//! # let mut spi = buffer.as_mut_slice();
//! // spi: something that implements the protocol transfer
//! let mut client = WincClient::new(spi);
//! let ssid = Ssid::from("ssid").unwrap();
//! let key = Credentials::from_wpa("password").unwrap();
//! nb::block!(client.start_wifi_module());
//! nb::block!(client.connect_to_ap(&ssid, &key, WifiChannel::ChannelAll, false));
//! nb::block!(client.get_host_by_name("google.com", AddrType::IPv4));
//! loop {
//!     client.heartbeat(); // periodically poll the chip
//! }
//! ```
//!
//! Code reference for this implementation is the [Arduino/Atmel Wifi101 library](https://docs.arduino.cc/libraries/wifi101)
//!
#![no_std]

#[cfg(feature = "std")]
extern crate std;

// Compile-time checks for logging features
#[cfg(all(feature = "defmt", feature = "log"))]
compile_error!("Features 'defmt' and 'log' are mutually exclusive. Enable only one for logging.");

#[cfg(not(any(feature = "defmt", feature = "log")))]
compile_error!("Must enable either 'defmt' or 'log' feature for logging support.");

#[cfg(feature = "defmt")]
pub(crate) use defmt::{debug, error, info, trace, warn};

#[cfg(feature = "log")]
pub(crate) use log::{debug, error, info, trace, warn};

mod client;
mod errors;
mod manager;
mod readwrite;
mod socket;
mod stack;
mod transfer;
pub use errors::CommError;
pub use transfer::Xfer as Transfer;

pub use client::PingResult;
pub use client::StackError;
pub use client::WincClient;
pub use manager::AuthType;
pub use manager::ConnectionInfo;
pub use manager::FirmwareInfo;
pub use manager::{
    AccessPoint, Credentials, HostName, S8Password, S8Username, SocketOptions, Ssid, UdpSockOpts,
    WifiChannel, WpaKey,
};
#[cfg(feature = "wep")]
pub use manager::{WepKey, WepKeyIndex};

// TODO: maybe don't expose this directly
pub use manager::ScanResult;

pub use client::Handle;

#[cfg(feature = "async")]
mod async_client;
#[cfg(feature = "async")]
pub use async_client::AsyncClient;

pub(crate) struct HexWrap<'a> {
    v: &'a [u8],
}
impl HexWrap<'_> {
    pub fn new(v: &[u8]) -> HexWrap<'_> {
        HexWrap { v }
    }
}
impl core::fmt::LowerHex for HexWrap<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        for elem in self.v {
            write!(f, " {:02x}", elem)?;
        }
        Ok(())
    }
}

#[cfg(feature = "defmt")]
impl<'a> defmt::Format for HexWrap<'a> {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(f, " bytes: {=[u8]:#x}", self.v)
    }
}

mod nonstd;
use nonstd::Ipv4AddrFormatWrapper;
