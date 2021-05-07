use std::convert::TryFrom;
use std::io;

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct Witness {
    data: Vec<u8>,
}

impl From<Vec<u8>> for Witness {
    fn from(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl AsRef<[u8]> for Witness {
    fn as_ref(&self) -> &[u8] {
        self.data.as_ref()
    }
}

impl AsMut<[u8]> for Witness {
    fn as_mut(&mut self) -> &mut [u8] {
        self.data.as_mut()
    }
}

impl Extend<u8> for Witness {
    fn extend<T: IntoIterator<Item = u8>>(&mut self, iter: T) {
        self.data.extend(iter);
    }
}

impl io::Read for Witness {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.len() < 8 + self.data.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "The provided buffer is not big enough!",
            ));
        }

        let len = (self.data.len() as u64).to_be_bytes();
        buf[..8].copy_from_slice(&len);
        buf[8..8 + self.data.len()].copy_from_slice(self.data.as_slice());

        Ok(8 + self.data.len())
    }
}

impl io::Write for Witness {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.len() < 8 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "The provided buffer is not big enough!",
            ));
        }

        let len = <[u8; 8]>::try_from(&buf[..8]).unwrap_or_else(|_| unreachable!());
        let len = u64::from_be_bytes(len) as usize;
        if len == 0 {
            self.data.clear();
            return Ok(8);
        }

        if buf.len() < 8 + len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "The provided buffer is not big enough!",
            ));
        }

        self.data = (&buf[8..8 + len]).to_vec();
        Ok(8 + self.data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
