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

use crate::readwrite::{Read, Write};

use crate::errors::Error;

/// Trait for reading and writing data
pub(crate) trait ReadWrite: Read + Write {}
impl<U> ReadWrite for U where U: Read + Write {}

/// Trait for transferring data to/from the WincWifi chip
///
/// There is an example SPI implementantion in demo crate.
pub trait Xfer {
    fn recv(&mut self, dest: &mut [u8]) -> Result<(), Error>;
    fn send(&mut self, src: &[u8]) -> Result<(), Error>;
}

// Blanket implementation
impl<U> Xfer for U
where
    U: ReadWrite,
{
    fn recv(&mut self, dest: &mut [u8]) -> Result<(), Error> {
        self.read_exact(dest).map_err(|_| Error::ReadError)
    }
    fn send(&mut self, src: &[u8]) -> Result<(), Error> {
        self.write(src).map_err(|_| Error::WriteError)?;
        Ok(())
    }
}
