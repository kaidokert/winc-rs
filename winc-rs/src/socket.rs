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

/// Default receive timeout (in milliseconds) for both TCP and UDP socket operations.
const DEFAULT_SOCKET_RECEIVE_TIMEOUT: u32 = 10_000;

/// Network Socket.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(PartialEq, Debug, Clone, Copy)]
pub struct Socket {
    /// Socket Identifier.
    pub v: u8, // todo make this not public
    /// Session Id.
    pub s: u16,
    /// Receive Timeout.
    receive_timeout: u32,
    /// SSL Configuration
    ssl_cfg: u8,
}

/// Implementation of `Socket` to create new instance and managing the receive timeout.
impl Socket {
    /// Creates a new instance of `Socket` with a default receive timeout (10,000 msecs).
    ///
    /// # Arguments
    ///
    /// * `v` - Socket identifier.
    /// * `s` - Session identifier.
    ///
    /// # Returns
    ///
    /// * `Socket` - A new `Socket` instance with default configuration.
    pub fn new(v: u8, s: u16) -> Self {
        Socket {
            v,
            s,
            receive_timeout: DEFAULT_SOCKET_RECEIVE_TIMEOUT,
            ssl_cfg: 0,
        }
    }

    /// Sets the receive timeout for the socket.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Timeout duration in milliseconds.
    pub(crate) fn set_recv_timeout(&mut self, timeout: u32) {
        self.receive_timeout = timeout;
    }

    /// Returns the receive timeout of the socket, in milliseconds.
    pub(crate) fn get_recv_timeout(&self) -> u32 {
        self.receive_timeout
    }

    /// Set the SSL Options
    ///
    /// # Arguments
    ///
    /// * `ssl_opt` - SSL socket options.
    pub(crate) fn set_ssl_cfg(&mut self, ssl_opt: u8, enable: bool) {
        if enable {
            self.ssl_cfg |= ssl_opt;
        } else {
            self.ssl_cfg &= !ssl_opt;
        }
    }

    /// Returns the SSL options value set on a socket.
    pub fn get_ssl_cfg(&self) -> u8 {
        self.ssl_cfg
    }
}

/// Creates a new `Socket` instance from a tuple of `(Socket ID, Session ID)`, with a default receive timeout (10,000 msecs).
impl From<(u8, u16)> for Socket {
    fn from(val: (u8, u16)) -> Self {
        Socket {
            v: val.0,
            s: val.1,
            receive_timeout: DEFAULT_SOCKET_RECEIVE_TIMEOUT,
            ssl_cfg: 0,
        }
    }
}

/// Converts a `Socket` into a tuple of `(Socket ID, Session ID)`.
impl From<Socket> for (u8, u16) {
    fn from(sock: Socket) -> Self {
        (sock.v, sock.s)
    }
}
