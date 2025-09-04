use crate::result::Result;
use bytes::Bytes;
use std::fmt::{
    Debug,
    Formatter,
};

/// Source from which blocks can be gathered for aggregation
pub trait BlockSource: Send + Sync {
    /// Asynchronously fetch the next block and its height
    fn next_block(&mut self) -> impl Future<Output = Result<(u64, Block)>> + Send;
}

#[derive(Clone, PartialEq, Eq)]
pub struct Block {
    bytes: Bytes,
}

#[cfg(test)]
impl Debug for Block {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        const BYTES_DISPLAY: usize = 8;
        if self.bytes.len() <= BYTES_DISPLAY {
            let bytes = &self
                .bytes
                .iter()
                .map(|b| format!("{}", b))
                .collect::<Vec<_>>();
            let bytes_string = bytes.join(", ");
            write!(f, "Block {{ bytes: [{}] }}", bytes_string)?;
        } else {
            let bytes_string = &self
                .bytes
                .iter()
                .take(BYTES_DISPLAY)
                .map(|b| format!("{}", b))
                .collect::<Vec<_>>();
            let bytes_string = bytes_string.join(", ");
            let len = self.bytes.len();
            write!(
                f,
                "Block {{ bytes: [{}, ...] (total {} bytes) }}",
                bytes_string, len
            )?;
        }
        Ok(())
    }
}

impl Block {
    pub fn new(bytes: Bytes) -> Self {
        Self { bytes }
    }

    #[cfg(test)]
    pub fn arb_size<Rng: rand::Rng + ?Sized>(rng: &mut Rng, size: usize) -> Self {
        let bytes: Bytes = (0..size).map(|_| rng.r#gen()).collect();
        Self::new(bytes)
    }

    #[cfg(test)]
    pub fn random<Rng: rand::Rng + ?Sized>(rng: &mut Rng) -> Self {
        const SIZE: usize = 100;
        Self::arb_size(rng, SIZE)
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl From<Vec<u8>> for Block {
    fn from(value: Vec<u8>) -> Self {
        let bytes = Bytes::from(value);
        Self::new(bytes)
    }
}
