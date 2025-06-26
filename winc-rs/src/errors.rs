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

/// Low-level chip communication errors
#[derive(Debug, PartialEq)]
pub enum CommError {
    Failed,
    BufferError,
    VectorCapacityError,
    // From where, which byte, expected, actual
    ProtocolByteError(&'static str, usize, u8, u8),
    ReadError,
    WriteError,
    BufferReadError,
    UnexpectedAddressFamily, // AF wasn't set to AF_INET in response,
    Utf8Error(core::str::Utf8Error),
    CapacityError(arrayvec::CapacityError),
    /// Wifi module boot rom start failed
    BootRomStart,
    /// Wifi module firmware failed to start
    FirmwareStart,
    /// HIF send failed
    HifSendFailed,
}

impl From<core::convert::Infallible> for CommError {
    fn from(_: core::convert::Infallible) -> Self {
        unreachable!("Infallible error")
    }
}

impl From<core::str::Utf8Error> for CommError {
    fn from(v: core::str::Utf8Error) -> Self {
        Self::Utf8Error(v)
    }
}

impl From<arrayvec::CapacityError> for CommError {
    fn from(v: arrayvec::CapacityError) -> Self {
        Self::CapacityError(v)
    }
}

impl From<BufferOverflow> for CommError {
    fn from(_: BufferOverflow) -> Self {
        CommError::BufferError
    }
}

impl<T> From<ReadExactError<T>> for CommError {
    fn from(_: ReadExactError<T>) -> Self {
        CommError::BufferReadError
    }
}

impl core::fmt::Display for CommError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let msg = match self {
            Self::Failed => "Operation failed",
            Self::BufferError => "Buffer error",
            Self::VectorCapacityError => "Vector capacity error",
            Self::ProtocolByteError(loc, byte, expected, actual) => {
                return write!(
                    f,
                    "Protocol byte error at {}, byte {}: expected {:#x}, got {:#x}",
                    loc, byte, expected, actual
                );
            }
            Self::ReadError => "Read error",
            Self::WriteError => "Write error",
            Self::BufferReadError => "Buffer read error",
            Self::UnexpectedAddressFamily => "Unexpected address family",
            Self::Utf8Error(err) => {
                return write!(
                    f,
                    "UTF-8 error: invalid sequence at position {}, error length: {:?}",
                    err.valid_up_to(),
                    err.error_len()
                )
            }
            Self::CapacityError(_) => return write!(f, "Capacity error: array full"),
            Self::BootRomStart => "WiFi module boot ROM start failed",
            Self::FirmwareStart => "WiFi module firmware start failed",
            Self::HifSendFailed => "HIF send failed",
        };
        f.write_str(msg)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for CommError {
    fn format(&self, f: defmt::Formatter) {
        match self {
            Self::Failed => defmt::write!(f, "Operation failed"),
            Self::BufferError => defmt::write!(f, "Buffer error"),
            Self::VectorCapacityError => defmt::write!(f, "Vector capacity error"),
            Self::ProtocolByteError(loc, byte, expected, actual) => {
                defmt::write!(
                    f,
                    "Protocol byte error at {}, byte {}: expected {:#x}, got {:#x}",
                    loc,
                    byte,
                    expected,
                    actual
                );
            }
            Self::ReadError => defmt::write!(f, "Read error"),
            Self::WriteError => defmt::write!(f, "Write error"),
            Self::BufferReadError => defmt::write!(f, "Buffer read error"),
            Self::UnexpectedAddressFamily => defmt::write!(f, "Unexpected address family"),
            Self::Utf8Error(err) => defmt::write!(
                f,
                "UTF-8 error: invalid sequence at position {}, error length: {:?}",
                err.valid_up_to(),
                err.error_len()
            ),
            Self::CapacityError(_) => defmt::write!(f, "Capacity error: array full"),
            Self::BootRomStart => defmt::write!(f, "WiFi module boot ROM start failed"),
            Self::FirmwareStart => defmt::write!(f, "WiFi module firmware start failed"),
            Self::HifSendFailed => defmt::write!(f, "HIF send failed"),
        }
    }
}
