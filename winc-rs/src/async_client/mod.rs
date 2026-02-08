use crate::stack::sock_holder::SocketStore;
use crate::stack::socket_callbacks::SocketCallbacks;
use crate::StackError;
use crate::{manager::Manager, transfer::Xfer, Handle};

use core::cell::RefCell;
use core::marker::PhantomData;
use core::ops::DerefMut;

mod dns;
mod module;
mod udp_stack;

pub struct AsyncClient<'a, X: Xfer> {
    manager: RefCell<Manager<X>>,
    callbacks: RefCell<SocketCallbacks>,
    next_session_id: RefCell<u16>,
    // Socket for UnconnectedUdp operations - kept alive across send/receive
    udp_socket: RefCell<Option<crate::Handle>>,
    _phantom: PhantomData<&'a ()>,
    #[cfg(test)]
    debug_callback: RefCell<Option<&'a mut dyn FnMut(&mut SocketCallbacks)>>,
}

impl<X: Xfer> AsyncClient<'_, X> {
    #[cfg(test)]
    const DNS_TIMEOUT: u32 = 50; // Shorter timeout for tests
    #[cfg(not(test))]
    const DNS_TIMEOUT: u32 = 1000;

    /// Maximum polling attempts for bind operations before timeout.
    /// Each poll yields to the executor, so actual time depends on executor scheduling.
    /// Hardware typically responds within a few polls under normal conditions.
    const BIND_MAX_POLLS: u32 = 1000;

    pub fn new(transfer: X) -> Self {
        Self {
            manager: RefCell::new(Manager::from_xfer(transfer)),
            callbacks: RefCell::new(SocketCallbacks::new()),
            next_session_id: RefCell::new(0),
            udp_socket: RefCell::new(None),
            _phantom: Default::default(),
            #[cfg(test)]
            debug_callback: RefCell::new(None),
        }
    }

    fn get_next_session_id(&self) -> u16 {
        let mut session_id = self.next_session_id.borrow_mut();
        let ret = *session_id;
        *session_id = session_id.wrapping_add(1);
        ret
    }

    fn dispatch_events(&self) -> Result<(), StackError> {
        #[cfg(test)]
        {
            let mut callbacks = self.debug_callback.borrow_mut();
            if let Some(callback) = callbacks.deref_mut() {
                let mut the_callbacks = self.callbacks.borrow_mut();
                callback(the_callbacks.deref_mut());
            }
        }
        let mut callbacks = self.callbacks.borrow_mut();
        let mut manager = self.manager.borrow_mut();
        manager
            .dispatch_events_new(callbacks.deref_mut())
            .map_err(StackError::DispatchError)
    }
    pub fn heartbeat(&self) -> Result<(), StackError> {
        self.dispatch_events()?;
        Ok(())
    }

    /// Yield control back to the async runtime, allowing other tasks to run.
    /// This should be called in polling loops to avoid busy-waiting.
    ///
    /// Note: This implementation uses wake_by_ref() which may cause the task to be
    /// re-polled quickly, but avoids adding external dependencies like `futures`.
    /// In practice, Embassy and other executors handle this scheduling reasonably well.
    /// For stricter yielding behavior, consider using runtime-specific APIs like
    /// `embassy_time::Timer::after_ticks(0)` or adding the `futures` crate dependency.
    async fn yield_once(&self) {
        use core::cell::Cell;

        // Stateful future that yields once: returns Pending on first poll, Ready on second
        let polled = Cell::new(false);
        core::future::poll_fn(|cx| {
            if polled.get() {
                // Second poll - return Ready to complete
                core::task::Poll::Ready(())
            } else {
                // First poll - mark as polled, wake ourselves, and return Pending
                polled.set(true);
                cx.waker().wake_by_ref();
                core::task::Poll::Pending
            }
        })
        .await
    }

    /// Close any existing UDP socket before creating a new one.
    /// Used by UdpStack trait implementations to ensure single-socket behavior.
    pub(crate) fn close_existing_udp_socket(&self) -> Result<(), StackError> {
        let mut socket_opt = self.udp_socket.borrow_mut();
        // Only proceed if there's a socket to close
        if socket_opt.is_some() {
            // Try to borrow manager and callbacks BEFORE taking the handle
            if let (Ok(mut manager), Ok(mut callbacks)) = (
                self.manager.try_borrow_mut(),
                self.callbacks.try_borrow_mut(),
            ) {
                // Now safe to take the handle since we have the borrows
                if let Some(handle) = socket_opt.take() {
                    if let Some((sock, _)) = callbacks.udp_sockets.get(handle) {
                        let socket_index =
                            sock.v as usize - crate::stack::socket_callbacks::NUM_TCP_SOCKETS;
                        manager.send_close(*sock)?;
                        callbacks.udp_sockets.remove(handle);
                        // Clear the address when closing socket
                        callbacks.udp_socket_connect_addr[socket_index] = None;
                    }
                }
            }
        }
        Ok(())
    }

    /// Allocate a new UDP socket handle.
    /// Used by UdpStack trait implementations.
    pub(crate) fn allocate_udp_socket(&self) -> Result<Handle, StackError> {
        let session_id = self.get_next_session_id();
        let mut callbacks = self.callbacks.borrow_mut();
        callbacks
            .udp_sockets
            .add(session_id)
            .ok_or(StackError::OutOfSockets)
    }

    /// Bind a socket to a specific port.
    /// Extracted from bind_udp() to be reusable by UdpStack trait implementations.
    pub(crate) async fn bind_socket_to_port(
        &self,
        handle: Handle,
        port: u16,
    ) -> Result<(), StackError> {
        crate::info!(
            "bind_socket_to_port: Binding socket {:?} to port {}",
            handle,
            port
        );

        // Create bind address (0.0.0.0:port)
        let bind_addr = core::net::SocketAddrV4::new(core::net::Ipv4Addr::UNSPECIFIED, port);

        // Set initial state and send bind request
        let socket = {
            let mut callbacks = self.callbacks.borrow_mut();
            let (sock, op) = callbacks
                .udp_sockets
                .get(handle)
                .ok_or(StackError::SocketNotFound)?;
            let socket = *sock;
            *op = crate::client::ClientSocketOp::Bind(None);
            socket
        };

        crate::info!(
            "bind_socket_to_port: Sending bind request for socket {:?}",
            socket
        );
        self.manager
            .borrow_mut()
            .send_bind(socket, bind_addr)
            .map_err(StackError::BindFailed)?;

        // Poll with attempt limit
        let mut poll_count = 0;

        loop {
            // Dispatch events to process hardware responses
            self.dispatch_events()?;

            // Check if bind completed
            let result = {
                let mut callbacks = self.callbacks.borrow_mut();
                let (_, op) = callbacks
                    .udp_sockets
                    .get(handle)
                    .ok_or(StackError::SocketNotFound)?;

                match op {
                    crate::client::ClientSocketOp::Bind(Some(bind_result)) => {
                        let error = bind_result.error;
                        *op = crate::client::ClientSocketOp::None;
                        Some(match error {
                            crate::manager::SocketError::NoError => {
                                crate::info!(
                                    "bind_socket_to_port: Bind successful after {} polls!",
                                    poll_count
                                );
                                Ok(())
                            }
                            error => {
                                crate::warn!(
                                    "bind_socket_to_port: Bind failed with error {:?}",
                                    error
                                );
                                Err(StackError::OpFailed(error))
                            }
                        })
                    }
                    _ => {
                        // Still waiting
                        None
                    }
                }
            };

            if let Some(result) = result {
                return result;
            }

            // Check timeout
            poll_count += 1;
            if poll_count >= Self::BIND_MAX_POLLS {
                let mut callbacks = self.callbacks.borrow_mut();
                if let Some((_, op)) = callbacks.udp_sockets.get(handle) {
                    *op = crate::client::ClientSocketOp::None;
                }
                crate::warn!("bind_socket_to_port: Timeout after {} polls", poll_count);
                return Err(StackError::GeneralTimeout);
            }

            // Yield to executor to allow other tasks and hardware processing
            self.yield_once().await;
        }
    }

    /// Get the actual local IP address from the connection state.
    /// Used by UdpStack trait implementations to resolve 0.0.0.0 addresses.
    pub(crate) fn get_actual_local_ip(
        &self,
        port: u16,
    ) -> Result<core::net::SocketAddrV4, StackError> {
        let callbacks = self.callbacks.borrow();
        let ip = callbacks
            .connection_state
            .ip_conf
            .as_ref()
            .map(|conf| conf.ip)
            .ok_or(StackError::InvalidState)?;
        Ok(core::net::SocketAddrV4::new(ip, port))
    }

    #[cfg(test)]
    pub(crate) fn set_unit_test_mode(&self) {
        self.manager.borrow_mut().set_unit_test_mode();
    }
}

/// Reference view wrapper for implementing `UdpStack` trait.
///
/// This newtype captures the lifetime in the stack type itself, allowing
/// socket wrappers to hold plain references without requiring GATs in the trait.
///
/// # Example
/// ```no_run
/// use embedded_nal_async::{ConnectedUdp, UdpStack};
/// use wincwifi::{AsyncClient, ClientStack};
///
/// # async fn example<X: wincwifi::Transfer>(transfer: X) -> Result<(), wincwifi::StackError> {
/// let client = AsyncClient::new(transfer);
/// let stack = ClientStack(&client);
/// let remote_addr = ([8, 8, 8, 8], 53).into();
/// let (local, mut socket) = stack.connect(remote_addr).await?;
/// socket.send(b"data").await?;
/// # Ok(())
/// # }
/// ```
pub struct ClientStack<'a, X: Xfer>(pub &'a AsyncClient<'a, X>);

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) struct MockTransfer {}

    impl Default for MockTransfer {
        fn default() -> Self {
            Self {}
        }
    }

    impl Xfer for MockTransfer {
        fn recv(&mut self, _: &mut [u8]) -> Result<(), crate::errors::CommError> {
            Ok(())
        }
        fn send(&mut self, _: &[u8]) -> Result<(), crate::errors::CommError> {
            Ok(())
        }
    }

    pub(crate) fn make_test_client<'a>() -> AsyncClient<'a, MockTransfer> {
        let client = AsyncClient::new(MockTransfer::default());
        client.set_unit_test_mode();
        client
    }
}
