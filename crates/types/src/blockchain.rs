//! Blockchain related types

use crate::blockchain::{
    block::Block,
    consensus::Sealed,
    header::BlockHeader,
};

pub mod block;
pub mod consensus;
pub mod header;
pub mod primitives;

/// Block header and the associated consensus info
pub type SealedBlockHeader = Sealed<BlockHeader>;

/// Block and the associated consensus info
pub type SealedBlock = Sealed<Block>;
