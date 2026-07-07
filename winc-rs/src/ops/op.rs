use crate::transfer::Xfer;
#[cfg(feature = "async")]
use crate::StackError;
#[cfg(feature = "async")]
use core::cell::RefCell;

/// Generic operation trait for network operations (DNS, TCP, UDP, etc.)
///
/// This trait provides a common interface for all network operations that need to
/// interact with the WINC manager and callbacks. Operations implement `poll_impl`
/// to perform their specific logic while receiving direct mutable references to
/// the required resources.
///
/// # Type Parameters
/// * `X` - The transfer implementation type
///
/// # Associated Types
/// * `Output` - The successful result type for this operation
/// * `Error` - The error type this operation can produce
pub trait OpImpl<X: Xfer> {
    type Output;
    type Error;

    /// Poll the operation for completion
    ///
    /// This method is called repeatedly until the operation completes or fails.
    /// It receives direct mutable access to the manager and callbacks, allowing
    /// efficient operation without RefCell overhead in sync contexts.
    ///
    /// # Parameters
    /// * `manager` - Mutable reference to the WINC manager
    /// * `callbacks` - Mutable reference to the socket callbacks
    ///
    /// # Returns
    /// * `Ok(Some(output))` - Operation completed successfully
    /// * `Ok(None)` - Operation is still in progress (would block)
    /// * `Err(error)` - Operation failed
    fn poll_impl(
        &mut self,
        manager: &mut crate::manager::Manager<X>,
        callbacks: &mut crate::stack::socket_callbacks::SocketCallbacks,
    ) -> Result<Option<Self::Output>, Self::Error>;
}

/// Generic Future wrapper for any operation implementing OpImpl
///
/// This struct provides a Future implementation that can wrap any operation
/// implementing the `OpImpl` trait, making it usable in async contexts.
/// The wrapper handles event dispatching and manages RefCell borrows automatically.
///
/// # Type Parameters
/// * `Op` - The operation type implementing `OpImpl<X>`
/// * `X` - The transfer implementation type
/// * `F` - The dispatch events closure type
#[cfg(feature = "async")]
pub struct AsyncOp<'a, Op, X: Xfer, F>
where
    Op: OpImpl<X>,
{
    op: Op,
    manager: &'a RefCell<crate::manager::Manager<X>>,
    callbacks: &'a RefCell<crate::stack::socket_callbacks::SocketCallbacks>,
    dispatch_events: F,
    waker: Option<core::task::Waker>,
}

#[cfg(feature = "async")]
impl<'a, Op, X: Xfer, F> AsyncOp<'a, Op, X, F>
where
    Op: OpImpl<X>,
{
    /// Create a new async operation wrapper
    ///
    /// # Parameters
    /// * `op` - The operation to wrap (must implement `OpImpl<X>`)
    /// * `manager` - RefCell containing the WINC manager
    /// * `callbacks` - RefCell containing the socket callbacks
    /// * `dispatch_events` - Closure for dispatching events
    pub fn new(
        op: Op,
        manager: &'a RefCell<crate::manager::Manager<X>>,
        callbacks: &'a RefCell<crate::stack::socket_callbacks::SocketCallbacks>,
        dispatch_events: F,
    ) -> Self {
        Self {
            op,
            manager,
            callbacks,
            dispatch_events,
            waker: None,
        }
    }
}

#[cfg(feature = "async")]
impl<Op, X: Xfer, F> core::future::Future for AsyncOp<'_, Op, X, F>
where
    Op: OpImpl<X> + Unpin,
    Op::Error: From<StackError>,
    F: Fn() -> Result<(), StackError> + Unpin,
{
    type Output = Result<Op::Output, Op::Error>;

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let this = self.get_mut();

        // Only update waker registration if the new waker is different from the stored one
        let new_waker = cx.waker();
        let waker_changed = this
            .waker
            .as_ref()
            .is_none_or(|old_waker| !old_waker.will_wake(new_waker));

        if waker_changed {
            // Unregister previous waker if we had one
            if let Some(ref old_waker) = this.waker {
                this.manager.borrow_mut().unregister_waker(old_waker);
            }

            // Register new waker with manager
            let new_waker_cloned = new_waker.clone();
            if let Err(e) = this
                .manager
                .borrow_mut()
                .register_waker(new_waker_cloned.clone())
            {
                // Waker array is full - too many concurrent async operations
                return core::task::Poll::Ready(Err(e.into()));
            }

            // Store waker for later unregistration
            this.waker = Some(new_waker_cloned);
        }

        // Dispatch events first
        if let Err(e) = (this.dispatch_events)() {
            // Unregister waker on error
            if let Some(ref waker) = this.waker {
                this.manager.borrow_mut().unregister_waker(waker);
            }
            this.waker = None;
            return core::task::Poll::Ready(Err(e.into()));
        }

        // Use the operation's trait implementation!
        let mut manager = this.manager.borrow_mut();
        let mut callbacks = this.callbacks.borrow_mut();

        match this.op.poll_impl(&mut manager, &mut callbacks) {
            Ok(Some(result)) => {
                // Unregister waker on completion
                if let Some(ref waker) = this.waker {
                    manager.unregister_waker(waker);
                }
                this.waker = None;
                core::task::Poll::Ready(Ok(result))
            }
            Ok(None) => {
                // Return Pending - waker is registered with manager
                // and will be woken when hardware events are processed
                core::task::Poll::Pending
            }
            Err(e) => {
                // Unregister waker on error
                if let Some(ref waker) = this.waker {
                    manager.unregister_waker(waker);
                }
                this.waker = None;
                core::task::Poll::Ready(Err(e))
            }
        }
    }
}

#[cfg(feature = "async")]
impl<Op, X: Xfer, F> Drop for AsyncOp<'_, Op, X, F>
where
    Op: OpImpl<X>,
{
    fn drop(&mut self) {
        // Unregister any stored waker when the future is dropped while pending
        // This prevents the manager from retaining stale wakers that will never be used again
        if let Some(waker) = self.waker.take() {
            // Try to unregister the waker, but ignore any errors to keep Drop infallible
            // If try_borrow_mut fails (manager already borrowed), we can't unregister,
            // but that's acceptable - Drop must not panic
            if let Ok(mut manager) = self.manager.try_borrow_mut() {
                manager.unregister_waker(&waker);
            }
        }
    }
}
