use core::convert::Infallible;
use wincwifi::StackError;

/// Error codes for initializing the peripherals of the Feather board.
#[derive(Debug, defmt::Format)]
pub enum FailureSource {
    Periph,
    Core,
    Clock,
}

/// Error codes used in example programs, combining Feather initialization
/// errors and WINC library errors.
#[derive(Debug, defmt::Format)]
pub enum AppError {
    BoardError(FailureSource),
    WincError(StackError),
}

/// Converts `FailureSource` error codes into `AppError`.
impl From<FailureSource> for AppError {
    fn from(err: FailureSource) -> Self {
        AppError::BoardError(err)
    }
}

/// Converts `StackError` error codes into `AppError`.
impl From<StackError> for AppError {
    fn from(err: StackError) -> Self {
        AppError::WincError(err)
    }
}

impl From<Infallible> for FailureSource {
    fn from(_: Infallible) -> Self {
        todo!()
    }
}
