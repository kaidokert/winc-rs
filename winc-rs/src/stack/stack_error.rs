use crate::manager::WifiConnError;

#[cfg(feature = "ota")]
use crate::manager::OtaUpdateError;
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
    DispatchError(crate::errors::CommError),
    ConnectSendFailed(crate::errors::CommError),
    ReceiveFailed(crate::errors::CommError),
    SendSendFailed(crate::errors::CommError),
    SendCloseFailed(crate::errors::CommError),
    BindFailed(crate::errors::CommError),
    WincWifiFail(crate::errors::CommError),
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
    #[cfg(feature = "ota")]
    /// Ota Error
    OtaFail(OtaUpdateError),
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

impl From<crate::errors::CommError> for StackError {
    fn from(inner: crate::errors::CommError) -> Self {
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
            Self::WouldBlock => write!(f, "Operation would block"),
            Self::GeneralTimeout => write!(f, "General timeout"),
            Self::ConnectTimeout => write!(f, "TCP connection timed out"),
            Self::RecvTimeout => write!(f, "Receive timeout"),
            Self::SendTimeout => write!(f, "Send timeout"),
            Self::OutOfSockets => write!(f, "Out of sockets"),
            Self::SocketAlreadyInUse => write!(f, "Socket already in use"),
            Self::CloseFailed => write!(f, "Close failed"),
            Self::Unexpected => write!(f, "Unexpected error"),
            Self::DispatchError(err) => write!(f, "Dispatch error: {}", err),
            Self::ConnectSendFailed(err) => write!(f, "Connect send failed: {}", err),
            Self::ReceiveFailed(err) => write!(f, "Receive failed: {}", err),
            Self::SendSendFailed(err) => write!(f, "Send send failed: {}", err),
            Self::SendCloseFailed(err) => write!(f, "Send close failed: {}", err),
            Self::BindFailed(err) => write!(f, "Bind failed: {}", err),
            Self::WincWifiFail(err) => write!(f, "WincWifi fail: {}", err),
            Self::OpFailed(err) => write!(f, "Operation failed: {}", err),
            Self::DnsTimeout => write!(f, "DNS lookup timed out"),
            Self::DnsFailed => write!(f, "DNS lookup failed"),
            Self::InvalidState => write!(f, "Invalid state"),
            Self::AlreadyConnected => write!(f, "Already connected"),
            Self::ApJoinFailed(err) => write!(f, "Access point join failed: {}", err),
            Self::ApScanFailed(err) => write!(f, "Access point scan failed: {}", err),
            Self::ContinueOperation => write!(f, "Continue operation"),
            Self::SocketNotFound => write!(f, "Socket not found"),
            Self::InvalidParameters => write!(f, "Invalid parameters"),
            #[cfg(feature = "ota")]
            Self::OtaFail(err) => write!(f, "Ota failure: {:?}", err),
        }
    }
}
