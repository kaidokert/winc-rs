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

use crate::readwrite::{BufferOverflow, ReadExactError};
use crate::StrError;
use arrayvec::CapacityError;

/// Low-level chip communication errors
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq)]
pub enum Error {
    Failed,
    BufferError,
    VectorCapacityError,
    // From where, which byte, expected, actual
    ProtocolByteError(&'static str, usize, u8, u8),
    ReadError,
    WriteError,
    BufferReadError,
    UnexpectedAddressFamily, // AF wasn't set to AF_INET in response,
    Str(StrError),
    /// Wifi module boot rom start failed
    BootRomStart,
    /// Wifi module firmware failed to start
    FirmwareStart,
    /// HIF send failed
    HifSendFailed,
}

impl From<core::convert::Infallible> for Error {
    fn from(_: core::convert::Infallible) -> Self {
        unreachable!("Infallible error")
    }
}

impl From<StrError> for Error {
    fn from(v: StrError) -> Self {
        Self::Str(v)
    }
}

impl From<core::str::Utf8Error> for Error {
    fn from(v: core::str::Utf8Error) -> Self {
        Self::Str(StrError::Utf8Error(v))
    }
}

impl From<BufferOverflow> for Error {
    fn from(_: BufferOverflow) -> Self {
        Error::BufferError
    }
}

impl From<CapacityError> for Error {
    fn from(_: CapacityError) -> Self {
        Error::VectorCapacityError
    }
}

impl<T> From<ReadExactError<T>> for Error {
    fn from(_: ReadExactError<T>) -> Self {
        Error::BufferReadError
    }
}
