use crate::{
    blocks::{
        Block,
        importer_and_db_source::BlockSerializer,
    },
    result::Error,
};

use anyhow::anyhow;
use fuel_core_types::blockchain::block::Block as FuelBlock;
use postcard::to_allocvec;

#[derive(Clone)]
pub struct SerializerAdapter;

impl BlockSerializer for SerializerAdapter {
    fn serialize_block(&self, block: &FuelBlock) -> crate::result::Result<Block> {
        let bytes_vec = to_allocvec(block).map_err(|e| {
            Error::BlockSource(anyhow!("failed to serialize block: {}", e))
        })?;
        Ok(crate::blocks::Block::from(bytes_vec))
    }
}
