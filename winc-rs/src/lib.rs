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

// TODO: High-level file comment.

#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(not(feature = "std"))]
use arrayvec::ArrayString;
#[cfg(not(feature = "std"))]
pub use defmt::{debug, error, info, trace, warn};

#[cfg(feature = "std")]
pub use log::{debug, error, info, trace, warn};

mod client;
pub mod error;
pub mod manager;
pub mod socket;
pub mod transfer;

#[cfg(not(feature = "std"))]
use core::fmt::Write;
pub use socket::Socket;

pub use no_std_net::{Ipv4Addr, SocketAddrV4};

pub use client::{Handle, WincClient, ClientSocketOp};
pub use client::SockHolder;

pub mod wifi;

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

#[cfg(not(feature = "std"))]
fn display_to_defmt<T: core::fmt::Display>(f: defmt::Formatter, v: &T) {
    let mut x = ArrayString::<40>::default();
    write!(&mut x, "{}", v).ok();
    defmt::write!(f, "{}", &x as &str)
}

#[cfg(not(feature = "std"))]
impl defmt::Format for StrError {
    fn format(&self, f: defmt::Formatter) {
        match self {
            Self::Utf8Error(e) => display_to_defmt(f, e),
            Self::CapacityError(e) => display_to_defmt(f, e),
        }
    }
}

#[cfg(not(feature = "std"))]
pub mod nonstd;
#[cfg(not(feature = "std"))]
pub use nonstd::Ipv4AddrFormatWrapper;
