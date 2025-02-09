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

use core::net::Ipv4Addr;

#[derive(Debug)]
pub struct Ipv4AddrFormatWrapper<'a> {
    #[allow(unused)]
    ip: &'a Ipv4Addr,
}

impl<'a> Ipv4AddrFormatWrapper<'a> {
    pub fn new(ip: &'a Ipv4Addr) -> Self {
        Self { ip }
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for Ipv4AddrFormatWrapper<'_> {
    fn format(&self, f: defmt::Formatter) {
        let ip: u32 = (*self.ip).into();
        defmt::write!(
            f,
            "{=u8}.{=u8}.{=u8}.{=u8}",
            ((ip >> 24) & 0xFF) as u8,
            ((ip >> 16) & 0xFF) as u8,
            ((ip >> 8) & 0xFF) as u8,
            ((ip >> 0) & 0xFF) as u8,
        );
    }
}
