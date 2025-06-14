// Copyright 2025 Google LLC
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

use crate::manager::Manager;
use crate::manager::SocketError;
use crate::socket::Socket;
use crate::transfer::Xfer;

mod dns;
mod prng;
mod tcp_stack;
mod udp_stack;
mod wifi_module;

pub use crate::stack::StackError;

pub use crate::stack::socket_callbacks::ClientSocketOp;
use crate::stack::socket_callbacks::SocketCallbacks;
pub use crate::stack::socket_callbacks::{Handle, PingResult};

use crate::stack::socket_callbacks::{AsyncOp, AsyncState};

use embedded_nal::nb;

use crate::stack::sock_holder::SocketStore;

/// Client for the WincWifi chip.
///
/// This manages the state of the chip and
/// network connections
pub struct WincClient<'a, X: Xfer> {
    manager: Manager<X>,
    poll_loop_delay_us: u32,
    callbacks: SocketCallbacks,
    next_session_id: u16,
    boot: Option<crate::manager::BootState>,
    operation_countdown: u32,
    phantom: core::marker::PhantomData<&'a ()>,
    #[cfg(test)]
    debug_callback: Option<&'a mut dyn FnMut(&mut SocketCallbacks)>,
}

impl<X: Xfer> WincClient<'_, X> {
    // Max send frame length
    #[cfg(not(test))]
    const MAX_SEND_LENGTH: usize = 1400;

    #[cfg(test)]
    const MAX_SEND_LENGTH: usize = 4;

    const TCP_SOCKET_BACKLOG: u8 = 4;
    const LISTEN_TIMEOUT: u32 = 100;
    const BIND_TIMEOUT: u32 = 100;
    // This only impacts for interval for loops, but doesn't actually
    // cause timeouts, as all calls are non-blocking.
    const RECV_TIMEOUT: u32 = 10_000;
    const CONNECT_TIMEOUT: u32 = 1000;
    const DNS_TIMEOUT: u32 = 1000;
    const POLL_LOOP_DELAY_US: u32 = 100;
    /// Create a new WincClient..
    ///
    /// # Arguments
    ///
    /// * `transfer` - The transfer implementation to use for client,
    ///   typically a struct wrapping SPI communication.
    ///
    ///  See [Xfer] for details how to implement a transfer struct.
    pub fn new(transfer: X) -> Self {
        let manager = Manager::from_xfer(transfer);
        Self {
            manager,
            callbacks: SocketCallbacks::new(),
            poll_loop_delay_us: Self::POLL_LOOP_DELAY_US,
            next_session_id: 0,
            boot: None,
            operation_countdown: 0,
            phantom: core::marker::PhantomData,
            #[cfg(test)]
            debug_callback: None,
        }
    }
    // Todo: remove this
    fn delay_us(&mut self, delay: u32) {
        self.manager.delay_us(delay)
    }

    #[cfg(feature = "irq")]
    pub fn get_irq_info(&mut self) -> crate::manager::IrqInfo {
        self.manager.irq_info.clone()
    }

    fn get_next_session_id(&mut self) -> u16 {
        let ret = self.next_session_id;
        self.next_session_id += 1;
        ret
    }

    fn test_hook(&mut self) {
        #[cfg(test)]
        if let Some(callback) = &mut self.debug_callback {
            callback(&mut self.callbacks);
        }
    }

    /// Poll the chip for new events.
    ///
    /// # Returns
    ///
    /// * `()` - No error occurred while polling the chip for new events.
    /// * `StackError` - An error occurred while polling the chip for new events.
    fn dispatch_events(&mut self) -> Result<(), StackError> {
        self.test_hook();
        self.manager
            .dispatch_events_new(&mut self.callbacks)
            .map_err(StackError::DispatchError)
    }

    /// Poll the chip for new events. If "irq" is enabled, it will wait for an interrupt on the IRQ
    /// pin of the WiFi chip before polling for new events. If "irq" is not enabled,
    /// it will poll the chip for new events without waiting.
    ///
    /// # Returns
    ///
    /// * `()` - No error occurred while polling for new events.
    /// * `StackError` - An error occurred while polling for new events.
    fn dispatch_events_may_wait(&mut self) -> Result<(), StackError> {
        self.test_hook();
        self.manager
            .dispatch_events_may_wait(&mut self.callbacks)
            .map_err(StackError::DispatchError)
    }

    fn wait_with_timeout<F, T>(
        &mut self,
        timeout: u32,
        mut check_complete: F,
    ) -> Result<T, StackError>
    where
        F: FnMut(&mut Self, u32) -> Option<Result<T, StackError>>,
    {
        self.dispatch_events()?;
        let mut timeout = timeout as i32;
        let mut elapsed = 0;

        loop {
            if timeout <= 0 {
                return Err(StackError::GeneralTimeout);
            }

            if let Some(result) = check_complete(self, elapsed) {
                return result;
            }

            self.delay_us(self.poll_loop_delay_us);
            self.dispatch_events()?;
            timeout -= self.poll_loop_delay_us as i32;
            elapsed += self.poll_loop_delay_us;
        }
    }

    // Todo: Too many arguments: poll delay should be removable
    // General async op state machine, with closures for init and complete
    #[allow(clippy::too_many_arguments)]
    fn async_op<T>(
        tcp: bool,
        socket: &Handle,
        callbacks: &mut SocketCallbacks,
        manager: &mut Manager<X>,
        poll_delay: u32,
        matcher: impl Fn(&AsyncOp) -> bool,
        init_callback: impl FnOnce(&Socket, &mut Manager<X>) -> Result<ClientSocketOp, StackError>,
        complete_callback: impl FnOnce(
            &Socket,
            &mut Manager<X>,
            &[u8],
            &mut AsyncOp,
        ) -> Result<T, StackError>,
    ) -> Result<T, nb::Error<StackError>> {
        let store: &mut dyn SocketStore = if tcp {
            &mut callbacks.tcp_sockets
        } else {
            &mut callbacks.udp_sockets
        };

        let (sock, op) = store.get(*socket).ok_or(StackError::SocketNotFound)?;
        match op {
            ClientSocketOp::None | ClientSocketOp::New => {
                *op = init_callback(sock, manager)?;
                manager
                    .dispatch_events_may_wait(callbacks)
                    .map_err(StackError::DispatchError)?;
                Err(nb::Error::WouldBlock)
            }
            ClientSocketOp::AsyncOp(asyncop, AsyncState::Pending(timeout_option))
                if matcher(asyncop) =>
            {
                manager.delay_us(poll_delay);
                if let Some(timeout) = timeout_option {
                    *timeout -= 1;
                    if *timeout == 0 {
                        Err(nb::Error::Other(StackError::OpFailed(SocketError::Timeout)))
                    } else {
                        manager
                            .dispatch_events_may_wait(callbacks)
                            .map_err(StackError::DispatchError)?;
                        Err(nb::Error::WouldBlock)
                    }
                } else {
                    manager
                        .dispatch_events_may_wait(callbacks)
                        .map_err(StackError::DispatchError)?;
                    Err(nb::Error::WouldBlock)
                }
            }
            ClientSocketOp::AsyncOp(asyncop, AsyncState::Done) if matcher(asyncop) => {
                let res = complete_callback(sock, manager, &callbacks.recv_buffer, asyncop);
                if let Err(StackError::ContinueOperation) = res {
                    *op = ClientSocketOp::AsyncOp(*asyncop, AsyncState::Pending(None));
                    Err(nb::Error::WouldBlock)
                } else {
                    *op = ClientSocketOp::None;
                    res.map_err(nb::Error::Other)
                }
            }
            _ => {
                unimplemented!("Unexpected async state: {:?}", op);
            }
        }
    }
}

#[cfg(test)]
mod test_shared {
    use super::*;

    pub(crate) struct MockTransfer {}

    impl Default for MockTransfer {
        fn default() -> Self {
            Self {}
        }
    }

    impl Xfer for MockTransfer {
        fn recv(&mut self, _: &mut [u8]) -> Result<(), crate::errors::Error> {
            Ok(())
        }
        fn send(&mut self, _: &[u8]) -> Result<(), crate::errors::Error> {
            Ok(())
        }
    }

    pub(crate) fn make_test_client<'a>() -> WincClient<'a, MockTransfer> {
        let mut client = WincClient::new(MockTransfer::default());
        client.manager.set_unit_test_mode();
        client
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_winc_client() {}
}
