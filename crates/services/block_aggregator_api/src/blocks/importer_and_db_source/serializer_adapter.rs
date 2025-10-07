use crate::{
    blocks::{
        Block,
        importer_and_db_source::BlockSerializer,
    },
    result::Error,
};

use crate::protobuf_types::Block as ProtoBlock;
use anyhow::anyhow;
use fuel_core_types::blockchain::block::Block as FuelBlock;
use postcard::to_allocvec;

#[derive(Clone)]
pub struct SerializerAdapter;

impl BlockSerializer for SerializerAdapter {
    type Block = ProtoBlock;

    fn serialize_block(&self, block: &FuelBlock) -> crate::result::Result<Self::Block> {
        let data = to_allocvec(block).map_err(|e| {
            Error::BlockSource(anyhow!("failed to serialize block: {}", e))
        })?;
        let pb = ProtoBlock { data };
        Ok(pb)
    }
}
