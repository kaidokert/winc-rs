use crate::manager::WifiConnError;

use crate::manager::SocketError;

use embedded_nal::nb;

/// Stack errors
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq)]
pub enum StackError {
    WouldBlock,
    GeneralTimeout,
    /// TCP connection timed out
    ConnectTimeout,
    RecvTimeout,
    SendTimeout,
    OutOfSockets,
    SocketAlreadyInUse,
    CloseFailed,
    Unexpected,
    DispatchError(crate::errors::Error),
    ConnectSendFailed(crate::errors::Error),
    ReceiveFailed(crate::errors::Error),
    SendSendFailed(crate::errors::Error),
    SendCloseFailed(crate::errors::Error),
    BindFailed(crate::errors::Error),
    WincWifiFail(crate::errors::Error),
    OpFailed(SocketError),
    /// DNS lookup timed out
    DnsTimeout,
    /// Unexpected DNS error
    DnsFailed,
    /// Operation was attempted in wrong state
    InvalidState,
    AlreadyConnected,
    /// Acess point join failed
    ApJoinFailed(WifiConnError),
    /// Scan operation failed
    ApScanFailed(WifiConnError),
    // Continue
    ContinueOperation,
    /// Not found
    SocketNotFound,
    /// Parameters are not valid.
    InvalidParameters,
}

impl From<core::convert::Infallible> for StackError {
    fn from(_: core::convert::Infallible) -> Self {
        unreachable!()
    }
}

impl From<SocketError> for StackError {
    fn from(inner: SocketError) -> Self {
        Self::OpFailed(inner)
    }
}

impl From<crate::errors::Error> for StackError {
    fn from(inner: crate::errors::Error) -> Self {
        Self::WincWifiFail(inner)
    }
}

impl embedded_nal::TcpError for StackError {
    fn kind(&self) -> embedded_nal::TcpErrorKind {
        embedded_nal::TcpErrorKind::Other
    }
}

impl From<nb::Error<StackError>> for StackError {
    fn from(inner: nb::Error<StackError>) -> Self {
        match inner {
            nb::Error::WouldBlock => StackError::WouldBlock,
            nb::Error::Other(e) => e,
        }
    }
}
