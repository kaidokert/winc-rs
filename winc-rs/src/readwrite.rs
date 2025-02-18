// Read+Write traits, copied from genio
// Todo: These aren't really that useful standalone, should merge this into transfer::ReadWrite

#[derive(Debug)]
pub enum ReadExactError<E> {
    Other(E),
    UnexpectedEnd,
}

impl<E> From<E> for ReadExactError<E> {
    fn from(e: E) -> Self {
        ReadExactError::Other(e)
    }
}

#[derive(Debug)]
pub struct BufferOverflow;

/// Trait for reading data from a source
///
/// Interface mimics std::io::Read
pub trait Read {
    type ReadError;
    fn available_bytes(&self, _at_least: usize) -> bool {
        true
    }
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::ReadError>;
    fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<(), ReadExactError<Self::ReadError>> {
        if self.available_bytes(buf.len()) {
            while !buf.is_empty() {
                let read_bytes = self.read(buf)?;
                if read_bytes == 0 {
                    return Err(ReadExactError::UnexpectedEnd);
                }

                let tmp = buf;
                buf = &mut tmp[read_bytes..];
            }
            Ok(())
        } else {
            Err(ReadExactError::UnexpectedEnd)
        }
    }
}

/// Trait for writing data to a destination
///
/// Interface mimics std::io::Write
pub trait Write {
    type WriteError;
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::WriteError>;
    fn write_all(&mut self, mut buf: &[u8]) -> Result<(), Self::WriteError> {
        while !buf.is_empty() {
            let len = self.write(buf)?;
            buf = &buf[len..];
        }
        Ok(())
    }
}

impl Read for &[u8] {
    type ReadError = void::Void;
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::ReadError> {
        use core::cmp::min;
        let amt = min(buf.len(), self.len());
        let (a, b) = self.split_at(amt);
        buf[..amt].copy_from_slice(a);
        *self = b;
        Ok(amt)
    }
    fn available_bytes(&self, at_least: usize) -> bool {
        self.len() >= at_least
    }
}

impl Read for &mut [u8] {
    type ReadError = void::Void;
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::ReadError> {
        let mut immutable: &[u8] = self;
        let amt = immutable.read(buf)?;
        *self = &mut core::mem::take(self)[amt..];
        Ok(amt)
    }
}

impl Write for &mut [u8] {
    type WriteError = BufferOverflow;
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::WriteError> {
        if buf.len() <= self.len() {
            let (first, second) = ::core::mem::take(self).split_at_mut(buf.len());
            first.copy_from_slice(&buf[0..buf.len()]);
            *self = second;
            Ok(buf.len())
        } else {
            Err(BufferOverflow)
        }
    }
}
