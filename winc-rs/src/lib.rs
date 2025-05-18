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
//! # use wincwifi::WincClient;
//! # use embedded_nal::{nb, AddrType, Dns};
//! # fn del_fn(ms: u32) {}
//! # let mut buffer = [0; 1];
//! # let mut spi = buffer.as_mut_slice();
//! // spi: something that implements the protocol transfer
//! let mut client = WincClient::new(spi);
//! nb::block!(client.start_wifi_module());
//! nb::block!(client.connect_to_ap("ssid", "password", false));
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

#[cfg(feature = "defmt")]
pub(crate) use defmt::{debug, error, info, trace, warn};
#[cfg(not(feature = "defmt"))]
pub(crate) use log::{debug, error, info, trace, warn};

mod client;
mod errors;
mod manager;
mod readwrite;
mod socket;
mod stack;
mod transfer;
pub use errors::Error as CommError;
pub use transfer::Xfer as Transfer;

pub use client::PingResult;
pub use client::StackError;
pub use client::WincClient;
pub use manager::AuthType;
pub use manager::ConnectionInfo;
pub use manager::FirmwareInfo;
pub use manager::{
    AccessPoint, Credentials, HostName, S8Password, S8Username, Ssid, WifiChannel, WpaKey,
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

// TODO: Merge this into CommError
#[derive(Debug, PartialEq)]
pub enum StrError {
    Utf8Error(core::str::Utf8Error),
    CapacityError(arrayvec::CapacityError),
}

impl From<core::str::Utf8Error> for StrError {
    fn from(v: core::str::Utf8Error) -> Self {
        Self::Utf8Error(v)
    }
}

impl From<arrayvec::CapacityError> for StrError {
    fn from(v: arrayvec::CapacityError) -> Self {
        Self::CapacityError(v)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for StrError {
    fn format(&self, f: defmt::Formatter) {
        match self {
            Self::Utf8Error(e) => defmt::write!(
                f,
                "UTF-8 error: invalid sequence at position {}, error length: {:?}",
                e.valid_up_to(),
                e.error_len()
            ),
            Self::CapacityError(_) => defmt::write!(f, "Capacity error: array full"),
        }
    }
}

pub(crate) struct HexWrap<'a> {
    v: &'a [u8],
}
impl HexWrap<'_> {
    pub fn new(v: &[u8]) -> HexWrap {
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
