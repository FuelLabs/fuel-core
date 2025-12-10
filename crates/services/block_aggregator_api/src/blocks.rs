use crate::result::Result;
use fuel_core_types::fuel_types::{
    BlockHeight,
    bytes::Bytes,
};
use std::fmt::Debug;

pub mod importer_and_db_source;

/// Source from which blocks can be gathered for aggregation
pub trait BlockSource: Send + Sync + 'static {
    type Block;

    /// Returns an iterator over blocks starting from the given block height
    fn blocks_starting_from(
        &self,
        block_height: BlockHeight,
    ) -> impl Iterator<Item = Result<(BlockHeight, Self::Block)>> + Send + Sync + 'static;
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct BlockBytes {
    bytes: Bytes,
}

impl BlockBytes {
    pub fn new(bytes: Bytes) -> Self {
        Self { bytes }
    }

    #[cfg(test)]
    pub fn arb_size<Rng: rand::Rng + ?Sized>(rng: &mut Rng, size: usize) -> Self {
        let bytes: Vec<u8> = (0..size).map(|_| rng.r#gen::<u8>()).collect();
        Self::new(bytes.into())
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

impl From<Vec<u8>> for BlockBytes {
    fn from(value: Vec<u8>) -> Self {
        let bytes = Bytes::from(value);
        Self::new(bytes)
    }
}
