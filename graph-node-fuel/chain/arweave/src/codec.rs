#[rustfmt::skip]
#[path = "protobuf/sf.arweave.r#type.v1.rs"]
mod pbcodec;

use graph::{blockchain::Block as BlockchainBlock, blockchain::BlockPtr, prelude::BlockNumber};

pub use pbcodec::*;

impl BlockchainBlock for Block {
    fn number(&self) -> i32 {
        BlockNumber::try_from(self.height).unwrap()
    }

    fn ptr(&self) -> BlockPtr {
        BlockPtr {
            hash: self.indep_hash.clone().into(),
            number: self.number(),
        }
    }

    fn parent_ptr(&self) -> Option<BlockPtr> {
        if self.height == 0 {
            return None;
        }

        Some(BlockPtr {
            hash: self.previous_block.clone().into(),
            number: self.number().saturating_sub(1),
        })
    }
}

impl AsRef<[u8]> for BigInt {
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}
