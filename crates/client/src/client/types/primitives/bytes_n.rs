use crate::client::types::primitives::{
    Len,
    Primitive,
};
use std::{
    fmt,
    slice::Chunks,
    str::FromStr,
};

#[derive(Debug, Clone, Default)]
pub struct BytesN(pub Vec<u8>);

impl fmt::LowerHex for BytesN {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <Self as Primitive>::fmt(self, f)
    }
}

impl AsRef<[u8]> for BytesN {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsMut<[u8]> for BytesN {
    fn as_mut(&mut self) -> &mut [u8] {
        self.0.as_mut()
    }
}

impl<T> From<T> for BytesN
where
    T: Into<Vec<u8>>,
{
    fn from(value: T) -> Self {
        let b: Vec<u8> = value.into();
        b.into()
    }
}

impl Primitive for BytesN {}

impl FromStr for BytesN {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        <Self as Primitive>::from_str(s)
    }
}

impl Len for BytesN {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn chunks(&self, chunk_size: usize) -> Chunks<u8> {
        self.0.chunks(chunk_size)
    }
}
