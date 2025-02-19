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

//! Winc Wifi library
//!
//! NOTE: This very much Work In Progress.
//! The main entry point is [WincClient].
//! Barebones [embedded_nal::TcpClientStack] and [embedded_nal::TcpClientStack]
//! are there, but not well tested.
//!
//! The low-lever library is in internal `manager`` module, it's the part that
//! wraps the HIF protocol and the chip registers.
//!
//! Connecting to AP, getting and IP, DNS lookups etc are implemented.
//!
//! Basic usage:
//! ```
//! // spi: something that implements the protocol transfer
//! // delay_fn: a callback function that lets the library wait
//! let mut client = WincClient::new(spi, delay_fn);
//! nb::block!(client.start_wifi_module());
//! nb::block!(client.connect_to_ap("ssid", "password"));
//! nb::block!(client.get_host_by_name("google.com", AddrType::IPv4));
//! loop {
//!     delay_fn(1000);
//!     client.heartbeat(); // periodically poll the chip
//! }
//! ```
#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "defmt")]
pub(crate) use defmt::{debug, error, info, trace, warn};
#[cfg(not(feature = "defmt"))]
pub(crate) use log::{debug, error, info, trace, warn};

mod client;
pub mod errors;
mod manager;
pub mod readwrite;
mod socket;
pub mod transfer;

pub use client::StackError;
pub use client::WincClient;
pub use manager::AuthType;
pub use manager::ConnectionInfo;
pub use manager::FirmwareInfo;

// TODO: maybe don't expose this directly
pub use manager::ScanResult;

// TODO: None of this should be public
pub use client::Handle;

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
