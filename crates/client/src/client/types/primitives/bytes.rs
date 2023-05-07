use crate::client::types::primitives::{
    Len,
    Primitive,
};
use core::{
    fmt,
    str::FromStr,
};
use std::slice::Chunks;

#[derive(Clone, Debug)]
pub struct Bytes<const N: usize>(pub [u8; N]);

impl<const N: usize> Bytes<N> {
    pub const fn new(bytes: [u8; N]) -> Self {
        Self(bytes)
    }

    pub const fn zeroed() -> Self {
        Self([0; N])
    }
}

impl<const N: usize> Default for Bytes<N> {
    fn default() -> Self {
        Self([0; N])
    }
}

impl<const N: usize> AsRef<[u8]> for Bytes<N> {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl<const N: usize> AsMut<[u8]> for Bytes<N> {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl<const N: usize> Len for Bytes<N> {
    fn len(&self) -> usize {
        N
    }

    fn chunks(&self, chunk_size: usize) -> Chunks<u8> {
        self.0.chunks(chunk_size)
    }
}

impl<const N: usize> Primitive for Bytes<N> {}

impl<const N: usize> fmt::LowerHex for Bytes<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <Self as Primitive>::fmt(self, f)
    }
}

impl<const N: usize> fmt::Display for Bytes<N> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        <Self as Primitive>::fmt(self, f)
    }
}

impl<const N: usize, T> From<T> for Bytes<N>
where
    T: Into<[u8; N]>,
{
    fn from(value: T) -> Self {
        let b: [u8; N] = value.into();
        b.into()
    }
}

impl<const N: usize> FromStr for Bytes<N> {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        <Self as Primitive>::from_str(s)
    }
}
