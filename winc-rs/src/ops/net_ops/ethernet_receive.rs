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

use crate::ops::op::OpImpl;
use crate::stack::StackError;
use crate::transfer::Xfer;

/// 1 second timeout to read an ethernet packet.
const ETHERNET_RX_TIMEOUT_MSEC: u32 = 1000;

pub(crate) struct RxEthernetPacketInfo<'a> {
    buffer: Option<&'a mut [u8]>,
    timeout: Option<u32>,
}

impl<'a> RxEthernetPacketInfo<'a> {
    pub(crate) fn new(buffer: Option<&'a mut [u8]>, timeout: Option<u32>) -> Self {
        Self { buffer, timeout }
    }
}

impl<X: Xfer> OpImpl<X> for RxEthernetPacketInfo<'_> {
    type Output = usize;
    type Error = StackError;

    /// Polls the internal state machine to receive an ethernet packet.
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
        callbacks: &mut crate::stack::socket_callbacks::SocketCallbacks,
    ) -> Result<Option<Self::Output>, Self::Error> {
        match &mut callbacks.eth_rx_info {
            None => {
                callbacks.eth_rx_info = Some(None);
                let timeout_ms = self.timeout.unwrap_or(ETHERNET_RX_TIMEOUT_MSEC);
                // todo clean-up
                manager.set_operation_timeout((timeout_ms * 1000) / 100);
            }
            Some(info) => {
                if let Some(info) = info {
                    let recv_buffer = match self.buffer.as_mut() {
                        None => {
                            callbacks.recv_buffer.fill(0);
                            callbacks.recv_buffer.as_mut_slice()
                        }
                        Some(buffer) => buffer,
                    };
                    // If no data is recevied from module, return early
                    if recv_buffer.is_empty() {
                        callbacks.eth_rx_info = None;
                        return Ok(Some(0));
                    }
                    let len_to_read = recv_buffer.len().min(info.packet_size as usize);
                    let rx_done = len_to_read >= info.packet_size as usize;
                    manager.recv_ethernet_packet(
                        info.hif_address + info.data_offset as u32,
                        &mut recv_buffer[..len_to_read],
                        rx_done,
                    )?;
                    // check if all data is read from the module.
                    if rx_done {
                        // no bytes left to read
                        callbacks.eth_rx_info = None;
                    } else {
                        info.data_offset += len_to_read as u16;
                        info.packet_size -= len_to_read as u16;
                    }

                    return Ok(Some(len_to_read));
                } else {
                    let mut timeout = manager.get_operation_timeout();
                    if timeout == 0 {
                        callbacks.eth_rx_info = None;
                        return Err(StackError::GeneralTimeout);
                    }
                    timeout -= 1;
                    manager.set_operation_timeout(timeout);
                }
            }
        }
        Ok(None)
    }
}
