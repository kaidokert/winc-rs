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

impl core::fmt::Display for StackError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StackError::WouldBlock => write!(f, "Operation would block"),
            StackError::GeneralTimeout => write!(f, "General timeout"),
            StackError::ConnectTimeout => write!(f, "TCP connection timed out"),
            StackError::RecvTimeout => write!(f, "Receive timeout"),
            StackError::SendTimeout => write!(f, "Send timeout"),
            StackError::OutOfSockets => write!(f, "Out of sockets"),
            StackError::SocketAlreadyInUse => write!(f, "Socket already in use"),
            StackError::CloseFailed => write!(f, "Close failed"),
            StackError::Unexpected => write!(f, "Unexpected error"),
            StackError::DispatchError(_) => write!(f, "Dispatch error"),
            StackError::ConnectSendFailed(_) => write!(f, "Connect send failed"),
            StackError::ReceiveFailed(_) => write!(f, "Receive failed"),
            StackError::SendSendFailed(_) => write!(f, "Send send failed"),
            StackError::SendCloseFailed(_) => write!(f, "Send close failed"),
            StackError::BindFailed(_) => write!(f, "Bind failed"),
            StackError::WincWifiFail(_) => write!(f, "WincWifi fail"),
            StackError::OpFailed(_) => write!(f, "Operation failed"),
            StackError::DnsTimeout => write!(f, "DNS lookup timed out"),
            StackError::DnsFailed => write!(f, "DNS lookup failed"),
            StackError::InvalidState => write!(f, "Invalid state"),
            StackError::AlreadyConnected => write!(f, "Already connected"),
            StackError::ApJoinFailed(_) => write!(f, "Access point join failed"),
            StackError::ApScanFailed(_) => write!(f, "Access point scan failed"),
            StackError::ContinueOperation => write!(f, "Continue operation"),
            StackError::SocketNotFound => write!(f, "Socket not found"),
            StackError::InvalidParameters => write!(f, "Invalid parameters"),
        }
    }
}
