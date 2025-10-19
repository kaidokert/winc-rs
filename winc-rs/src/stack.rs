pub mod constants;
pub mod sock_holder;
pub mod socket_callbacks;
/// Shared stack code for both sync and async clients
pub mod stack_error;

use crate::socket::Socket;
// Re-export MAX_SEND_LENGTH only when it's actually used
// Currently unused at crate level but may be needed for public API
#[allow(unused_imports)]
pub use constants::MAX_SEND_LENGTH;
#[cfg(test)]
pub use constants::MAX_SEND_LENGTH_TEST;
pub use sock_holder::SockHolder;
pub use socket_callbacks::ClientSocketOp;
use socket_callbacks::Handle;
pub use stack_error::StackError;
