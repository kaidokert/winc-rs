use crate::manager::Manager;
use crate::socket::Socket;
use crate::transfer::Xfer;

use crate::manager::SocketError;

use crate::debug;

mod dns;
mod tcp_stack;
mod udp_stack;
mod wifi_module;

pub use crate::stack::StackError;

pub use crate::stack::socket_callbacks::ClientSocketOp;
use crate::stack::socket_callbacks::SocketCallbacks;
pub use crate::stack::socket_callbacks::{Handle, PingResult};

pub enum GenResult {
    Len(usize),
    Accept(core::net::SocketAddrV4, Socket),
}

/// Client for the WincWifi chip.
///
/// This manages the state of the chip and
/// network connections
pub struct WincClient<'a, X: Xfer> {
    manager: Manager<X>,
    recv_timeout: u32,
    poll_loop_delay: u32,
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
    const MAX_SEND_LENGTH: usize = 1400;

    const TCP_SOCKET_BACKLOG: u8 = 4;
    const LISTEN_TIMEOUT: u32 = 100;
    const ACCEPT_TIMEOUT: u32 = 100;
    const BIND_TIMEOUT: u32 = 100;
    const SEND_TIMEOUT: u32 = 1000;
    const RECV_TIMEOUT: u32 = 1000;
    const CONNECT_TIMEOUT: u32 = 1000;
    const DNS_TIMEOUT: u32 = 1000;
    const POLL_LOOP_DELAY: u32 = 10;
    /// Create a new WincClient..
    ///
    /// # Arguments
    ///
    /// * `transfer` - The transfer implementation to use for client,
    ///             typically a struct wrapping SPI communication.
    ///
    ///  See [Xfer] for details how to implement a transfer struct.
    pub fn new(transfer: X) -> Self {
        let manager = Manager::from_xfer(transfer);
        Self {
            manager,
            callbacks: SocketCallbacks::new(),
            recv_timeout: Self::RECV_TIMEOUT,
            poll_loop_delay: Self::POLL_LOOP_DELAY,
            next_session_id: 0,
            boot: None,
            operation_countdown: 0,
            phantom: core::marker::PhantomData,
            #[cfg(test)]
            debug_callback: None,
        }
    }
    // Todo: remove this
    fn delay(&mut self, delay: u32) {
        // delegate to manager->chip->delay
        self.manager.delay(delay);
    }
    fn get_next_session_id(&mut self) -> u16 {
        let ret = self.next_session_id;
        self.next_session_id += 1;
        ret
    }
    fn dispatch_events(&mut self) -> Result<(), StackError> {
        #[cfg(test)]
        if let Some(callback) = &mut self.debug_callback {
            callback(&mut self.callbacks);
        }
        self.manager
            .dispatch_events_new(&mut self.callbacks)
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

            self.delay(self.poll_loop_delay);
            self.dispatch_events()?;
            timeout -= self.poll_loop_delay as i32;
            elapsed += self.poll_loop_delay;
        }
    }

    fn wait_for_op_ack(
        &mut self,
        handle: Handle,
        expect_op: ClientSocketOp,
        timeout: u32,
        tcp: bool,
    ) -> Result<GenResult, StackError> {
        self.callbacks.last_accept_addr = None;
        self.callbacks.last_error = SocketError::NoError;

        debug!("===>Waiting for op ack for {:?}", expect_op);

        self.wait_with_timeout(timeout, |client, elapsed| {
            let (_sock, op) = match tcp {
                true => client.callbacks.tcp_sockets.get(handle).unwrap(),
                false => client.callbacks.udp_sockets.get(handle).unwrap(),
            };

            if *op == ClientSocketOp::None {
                debug!(
                    "<===Ack received for {:?}, recv_len:{:?}, elapsed:{}ms",
                    expect_op, client.callbacks.recv_len, elapsed
                );

                if let Some(accepted_socket) = client.callbacks.last_accepted_socket.take() {
                    return Some(Ok(GenResult::Accept(
                        client.callbacks.last_accept_addr.unwrap(),
                        accepted_socket,
                    )));
                }

                if client.callbacks.last_error != SocketError::NoError {
                    return Some(Err(StackError::OpFailed(client.callbacks.last_error)));
                }
                return Some(Ok(GenResult::Len(client.callbacks.recv_len)));
            }
            None
        })
        .map_err(|e| {
            if matches!(e, StackError::GeneralTimeout) {
                match expect_op {
                    ClientSocketOp::Connect(_) => StackError::ConnectTimeout,
                    ClientSocketOp::Send(_) => StackError::SendTimeout,
                    ClientSocketOp::Recv => StackError::RecvTimeout,
                    _ => StackError::GeneralTimeout,
                }
            } else {
                e
            }
        })
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
