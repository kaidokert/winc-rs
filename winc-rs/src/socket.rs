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

#[cfg_attr(not(feature = "std"), derive(defmt::Format))]
#[derive(PartialEq, Debug, Clone, Copy)]
pub struct Socket {
    pub v: u8, // todo make this not public
    pub s: u16,
}
impl Socket {
    pub fn new(v: u8, s: u16) -> Self {
        Socket { v, s }
    }
}
impl From<(u8, u16)> for Socket {
    fn from(val: (u8, u16)) -> Self {
        Socket { v: val.0, s: val.1 }
    }
}
impl From<Socket> for (u8, u16) {
    fn from(sock: Socket) -> Self {
        (sock.v, sock.s)
    }
}
