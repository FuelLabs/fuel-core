use crate::blockchain::{
    block::Block,
    consensus::Sealed,
    header::BlockHeader,
};

pub mod block;
pub mod consensus;
pub mod header;
pub mod primitives;

pub type SealedBlockHeader = Sealed<BlockHeader>;
pub type SealedBlock = Sealed<Block>;
