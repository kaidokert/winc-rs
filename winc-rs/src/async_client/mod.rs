use crate::stack::socket_callbacks::SocketCallbacks;
use crate::StackError;
use crate::{manager::Manager, transfer::Xfer};

use core::cell::RefCell;
use core::marker::PhantomData;
use core::ops::DerefMut;

mod dns;
mod module;
mod udp_stack;

pub struct AsyncClient<'a, X: Xfer> {
    manager: RefCell<Manager<X>>,
    callbacks: RefCell<SocketCallbacks>,
    _phantom: PhantomData<&'a ()>,
    #[cfg(test)]
    debug_callback: RefCell<Option<&'a mut dyn FnMut(&mut SocketCallbacks)>>,
}

impl<X: Xfer> AsyncClient<'_, X> {
    #[cfg(test)]
    const DNS_TIMEOUT: u32 = 50; // Shorter timeout for tests
    #[cfg(not(test))]
    const DNS_TIMEOUT: u32 = 1000;

    pub fn new(transfer: X) -> Self {
        Self {
            manager: RefCell::new(Manager::from_xfer(transfer)),
            callbacks: RefCell::new(SocketCallbacks::new()),
            _phantom: Default::default(),
            #[cfg(test)]
            debug_callback: RefCell::new(None),
        }
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

    #[cfg(test)]
    pub(crate) fn set_unit_test_mode(&self) {
        self.manager.borrow_mut().set_unit_test_mode();
    }
}

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
