use embedded_nal::TcpFullStack;

pub enum ErrWrap<T, E>
where
    T: TcpFullStack<Error = E> + Sized,
    E: core::fmt::Debug,
{
    ErrWrap(T::Error),
}
impl<T, E> core::fmt::Debug for ErrWrap<T, E>
where
    T: TcpFullStack<Error = E> + Sized,
    E: core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ErrWrap::ErrWrap(e) => write!(f, "ErrWrap {:?}", e),
        }
    }
}

impl<T, E> embedded_io::Error for ErrWrap<T, E>
where
    T: TcpFullStack<Error = E> + Sized,
    E: core::fmt::Debug,
{
    fn kind(&self) -> embedded_io::ErrorKind {
        embedded_io::ErrorKind::Other
    }
}
impl<'a, T, E> From<E> for ErrWrap<T, E>
where
    T: TcpFullStack<Error = E> + Sized,
    E: core::fmt::Debug,
{
    fn from(err: E) -> Self {
        ErrWrap::ErrWrap(err)
    }
}
