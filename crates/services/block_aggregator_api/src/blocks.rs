use crate::result::Result;
use bytes::Bytes;
use fuel_core_types::fuel_types::BlockHeight;
use std::fmt::Debug;

/// Source from which blocks can be gathered for aggregation
pub trait BlockSource: Send + Sync {
    /// Asynchronously fetch the next block and its height
    fn next_block(&mut self)
    -> impl Future<Output = Result<(BlockHeight, Block)>> + Send;

    /// Drain any remaining blocks from the source
    fn drain(&mut self) -> impl Future<Output = Result<()>> + Send;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    bytes: Bytes,
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
