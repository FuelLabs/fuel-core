use fuel_core_types::fuel_types::BlockHeight;
use protobuf_types::Block as ProtoBlock;

pub mod api;
pub mod block_range_response;
pub mod blocks;
pub mod db;
pub mod protobuf_types;
pub mod result;
pub mod service;
pub mod task;

#[cfg(test)]
mod tests;

pub struct NewBlock {
    height: BlockHeight,
    block: ProtoBlock,
}

impl NewBlock {
    pub fn new(height: BlockHeight, block: ProtoBlock) -> Self {
        Self { height, block }
    }

    pub fn into_inner(self) -> (BlockHeight, ProtoBlock) {
        (self.height, self.block)
    }
}
