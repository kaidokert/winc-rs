// Copyright 2026 Google LLC
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

use crate::manager::PRNG_DATA_LENGTH;
use crate::ops::op::OpImpl;
use crate::stack::{
    socket_callbacks::{Prng, SocketCallbacks},
    StackError,
};
use crate::transfer::Xfer;

// 5 seconds max, assuming no additional delays
const PRNG_REQUEST_TIMEOUT_MILLISECONDS: u32 = 5_000;

/// A struct representing a random number generation operation.
pub(crate) struct PrngOp<'a> {
    data: &'a mut [u8],
}

/// Constructs a new `PrngOp` with the given data buffer.
impl<'a> PrngOp<'a> {
    pub(crate) fn new(data: &'a mut [u8]) -> Self {
        Self { data }
    }
}

/// Handles random number generation operation.
impl<'a, X: Xfer> OpImpl<X> for PrngOp<'a> {
    type Output = ();
    type Error = StackError;

    /// Polls the internal state machine and generates random data.
    ///
    /// # Arguments
    ///
    /// * `manager` - The stack manager handling low-level operations.
    /// * `callbacks` - Socket callback handlers.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(output))` - Operation completed successfully.
    /// * `Ok(None)` - Operation is still in progress.
    /// * `Err(Self::Error)` - An error occurred while polling.
    fn poll_impl(
        &mut self,
        manager: &mut crate::manager::Manager<X>,
        callbacks: &mut SocketCallbacks,
    ) -> Result<Option<Self::Output>, Self::Error> {
        if self.data.is_empty() {
            callbacks.prng = None;
            return Ok(Some(()));
        }
        match &mut callbacks.prng {
            None => {
                manager.set_operation_timeout(PRNG_REQUEST_TIMEOUT_MILLISECONDS);
                let to_recv = self.data.len().min(PRNG_DATA_LENGTH);
                let data = Prng {
                    offset: 0,
                    rcv_buffer: None,
                };
                manager.send_prng(self.data.as_ptr() as u32, to_recv as u16)?;
                callbacks.prng = Some(Some(data));
            }
            Some(op_prng) => {
                match op_prng {
                    Some(prng) => {
                        if let Some(rcv_buff) = prng.rcv_buffer {
                            let rcvd_len = rcv_buff.len().min((self.data[prng.offset..]).len());
                            // copy the buffer
                            self.data[prng.offset..(prng.offset + rcvd_len)]
                                .copy_from_slice(&rcv_buff[..rcvd_len]);
                            // add the offset
                            let offset = prng.offset + rcvd_len;
                            // check if total length is received
                            if offset >= self.data.len() {
                                callbacks.prng = None;
                                return Ok(Some(()));
                            } else {
                                // resend the command
                                let new_data = Prng {
                                    offset,
                                    rcv_buffer: None,
                                };
                                let to_recv = (self.data[offset..]).len().min(PRNG_DATA_LENGTH);
                                manager.send_prng(self.data.as_ptr() as u32, to_recv as u16)?;
                                callbacks.prng = Some(Some(new_data));
                            }
                        } else {
                            let timeout = manager.get_operation_timeout();
                            if timeout == 0 {
                                callbacks.prng = None;
                                return Err(StackError::GeneralTimeout);
                            }
                            manager.set_operation_timeout(timeout - 1);
                        }
                    }
                    _ => {
                        return Err(StackError::Unexpected);
                    }
                }
            }
        }

        Ok(None)
    }
}
