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

pub use genio::{Read, Write};

#[cfg(feature = "std")]
use std::{thread, time};

use crate::error::Error;
use arrayvec::{ArrayVec, CapacityError};

type TmpBuffer = ArrayVec<u8, 256>;

pub trait ReadWrite: Read + Write {}
impl<U> ReadWrite for U where U: Read + Write {}

fn concat<'a>(
    dest: &'a mut TmpBuffer,
    slice1: &[u8],
    slice2: &[u8],
) -> Result<&'a TmpBuffer, CapacityError> {
    dest.clear();
    dest.try_extend_from_slice(slice1)?;
    dest.try_extend_from_slice(slice2)?;
    Ok(dest)
}

pub trait Xfer {
    fn recv(&mut self, dest: &mut [u8]) -> Result<(), Error>;
    fn send(&mut self, src: &[u8]) -> Result<(), Error>;
}

// Debug implementation of Xfer. Prefixes read/write with a 3-byte header.
pub struct PrefixXfer<T: ReadWrite> {
    stream: T,
}
impl<T: ReadWrite> PrefixXfer<T> {
    pub fn new(stream: T) -> Self {
        PrefixXfer { stream }
    }
}

impl<T: ReadWrite> Xfer for PrefixXfer<T> {
    fn recv(&mut self, dest: &mut [u8]) -> Result<(), Error> {
        let rd_cmnd = [0xA2, 0x00, dest.len() as u8];
        self.stream.write(&rd_cmnd).map_err(|_| Error::WriteError)?;
        self.stream.read_exact(dest).map_err(|_| Error::ReadError)?;
        Ok(())
    }

    fn send(&mut self, src: &[u8]) -> Result<(), Error> {
        let wr_cmnd = [0x81, 00, src.len() as u8];
        let wr_slice = &wr_cmnd[..];
        let mut buf = TmpBuffer::new();
        concat(&mut buf, wr_slice, src)?;
        self.stream
            .write(buf.as_slice())
            .map_err(|_| Error::WriteError)?;

        #[cfg(feature = "std")]
        thread::sleep(time::Duration::from_millis(10));

        Ok(())
    }
}

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

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_concat() {
        let mut array = TmpBuffer::new();

        assert_eq!(
            concat(&mut array, &[1u8; 2], &[2u8; 3]).unwrap().as_slice(),
            &[1, 1, 2, 2, 2]
        );
    }
}
