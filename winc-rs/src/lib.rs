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
//! The low-lever library is in [manager] module, it's the part that wraps the
//! HIF protocol and the chip registers.
//!
//! Connecting to AP, getting and IP, DNS lookups etc are implemented.
//!
//!
#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "defmt")]
use arrayvec::ArrayString;

#[cfg(feature = "defmt")]
pub(crate) use defmt::{debug, error, info, trace, warn};
#[cfg(not(feature = "defmt"))]
pub(crate) use log::{debug, error, info, trace, warn};

mod client;
pub mod errors;
pub mod manager;
pub mod readwrite;
pub mod socket;
pub mod transfer;

#[cfg(feature = "defmt")]
use core::fmt::Write;

pub use client::StackError;
pub use client::WincClient;

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
fn display_to_defmt<T: core::fmt::Display>(f: defmt::Formatter, v: &T) {
    let mut x = ArrayString::<40>::default();
    write!(&mut x, "{}", v).ok();
    defmt::write!(f, "{}", &x as &str)
}

#[cfg(feature = "defmt")]
impl defmt::Format for StrError {
    fn format(&self, f: defmt::Formatter) {
        match self {
            Self::Utf8Error(e) => display_to_defmt(f, e),
            Self::CapacityError(e) => display_to_defmt(f, e),
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
pub use nonstd::Ipv4AddrFormatWrapper;
