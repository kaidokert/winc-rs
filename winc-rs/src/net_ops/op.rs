use crate::transfer::Xfer;

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
