use crate::client::{
    hex_formatted::hex_val,
    types::primitives::bytes::Bytes,
};
use std::{
    fmt,
    slice::Chunks,
};

mod bytes;
mod bytes_n;

trait Len {
    fn len(&self) -> usize;

    fn chunks(&self, chunk_size: usize) -> Chunks<u8>;
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

pub use bytes_n::BytesN;
pub type Bytes32 = Bytes<32>;
pub type Bytes64 = Bytes<64>;
