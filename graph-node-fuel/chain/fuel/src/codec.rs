#[rustfmt::skip]
#[path = "protobuf/sf.fuel.r#type.v1.rs"]
mod pbcodec;

use std::hash::Hash;
use graph::{
    blockchain::{
        BlockPtr,
        Block as BlockchainBlock
    },
    prelude::{anyhow::anyhow, BlockNumber, Error},
};

pub use pbcodec::*;

impl Block {

    pub fn parent_ptr(&self) -> Option<BlockPtr> {
        match self.height {
            0 => None,
            _ => Some(BlockPtr {
                hash: self.prev_root.clone().into(),
                number: self.number().saturating_sub(1),
            }),
        }
    }
}

impl<'a> From<&'a Block> for BlockPtr {
    fn from(b: &'a Block) -> BlockPtr {
        BlockPtr::new(
            b.id.clone().into(),
             b.number()
        )
    }
}

impl BlockchainBlock for Block {
    fn ptr(&self) -> BlockPtr {
        BlockPtr::try_from(self).unwrap()
    }

    fn parent_ptr(&self) -> Option<BlockPtr> {
        self.parent_ptr()
    }

    fn number(&self) -> i32 {
        BlockNumber::try_from(self.height).unwrap()
    }
}