use crate::client::hex_formatted::hex_val;
use core::{
    fmt,
    str::FromStr,
};
use std::slice::Chunks;

trait Len {
    fn len(&self) -> usize;

    fn chunks(&self, chunk_size: usize) -> Chunks<u8>;
}

#[derive(Clone, Debug)]
pub struct Bytes<const N: usize>(pub [u8; N]);

impl<const N: usize> Bytes<N> {
    pub const fn new(bytes: [u8; N]) -> Self {
        Self(bytes)
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

impl<const N: usize> fmt::Display for Bytes<N> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        <Self as fmt::LowerHex>::fmt(&self, f)
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

impl<const N: usize> fmt::LowerHex for Bytes<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

pub type Bytes32 = Bytes<32>;
pub type Bytes64 = Bytes<64>;

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

trait Primitive
where
    Self: Default + AsRef<[u8]> + AsMut<[u8]> + Len,
{
    fn from_str(s: &str) -> Result<Self, &'static str> {
        const ERR: &str = "Invalid encoded byte";

        let alternate = s.starts_with("0x");

        let mut b = s.bytes();
        let mut ret = Self::default();

        if alternate {
            b.next();
            b.next();
        }

        for r in ret.as_mut() {
            let h = b.next().and_then(hex_val).ok_or(ERR)?;
            let l = b.next().and_then(hex_val).ok_or(ERR)?;

            *r = h << 4 | l;
        }

        Ok(ret)
    }

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "0x")?
        }

        match f.width() {
            Some(w) if w > 0 => self.chunks(2 * self.len() / w).try_for_each(|c| {
                write!(f, "{:02x}", c.iter().fold(0u8, |acc, x| acc ^ x))
            }),

            _ => self
                .as_ref()
                .iter()
                .try_for_each(|b| write!(f, "{:02x}", &b)),
        }
    }
}

impl<const N: usize> Primitive for Bytes<N> {}
impl Primitive for BytesN {}
