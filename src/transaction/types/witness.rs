use crate::bytes;

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
        bytes::store_bytes(buf, self.data.as_slice()).map(|(n, _)| n)
    }
}

impl io::Write for Witness {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        bytes::restore_bytes(buf).map(|(n, data, _)| {
            self.data = data;
            n
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
