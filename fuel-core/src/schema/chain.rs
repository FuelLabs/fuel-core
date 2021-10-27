use crate::database::Database;
use crate::model::fuel_block::FuelBlock;
use crate::schema::block::Block;
use async_graphql::{Context, Object};
use fuel_storage::Storage;
use fuel_tx::Bytes32;

pub struct ChainInfo;

#[Object]
impl ChainInfo {
    async fn name(&self) -> String {
        "Fuel.testnet".to_string()
    }

    async fn latest_block(&self, ctx: &Context<'_>) -> async_graphql::Result<Block> {
        let db = ctx.data_unchecked::<Database>().clone();
        let height = db.get_block_height()?.unwrap_or_default();
        let id = db.get_block_id(height)?.unwrap_or_default();
        let block = Storage::<Bytes32, FuelBlock>::get(&db, &id)?.unwrap_or_default();
        Ok(Block(block.into_owned()))
    }

    async fn base_chain_height(&self) -> u64 {
        0
    }

    async fn peer_count(&self) -> u64 {
        0
    }
}

#[derive(Default)]
pub struct ChainQuery;

#[Object]
impl ChainQuery {
    async fn chain(&self) -> ChainInfo {
        ChainInfo
    }
}
