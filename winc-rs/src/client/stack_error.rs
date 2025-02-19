use crate::manager::WifiConnError;

use super::SocketError;

use embedded_nal::nb;

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug)]
pub enum StackError {
    WouldBlock,
    GeneralTimeout,
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
    GlobalOpFailed,
    DnsTimeout,
    DnsFailed,
    InvalidState,
    AlreadyConnected,
    ApJoinFailed(WifiConnError),
    ApScanFailed(WifiConnError),
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
