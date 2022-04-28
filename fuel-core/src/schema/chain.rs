use crate::database::Database;
use crate::model::FuelBlockDb;
use crate::schema::block::Block;
use crate::schema::scalars::U64;
use async_graphql::{Context, Object};
use fuel_storage::Storage;

pub const DEFAULT_NAME: &str = "Fuel.testnet";

pub struct ChainInfo;

#[Object]
impl ChainInfo {
    async fn name(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        let db = ctx.data_unchecked::<Database>().clone();
        let name = db
            .get_chain_name()?
            .unwrap_or_else(|| DEFAULT_NAME.to_string());
        Ok(name)
    }

    async fn latest_block(&self, ctx: &Context<'_>) -> async_graphql::Result<Block> {
        let db = ctx.data_unchecked::<Database>().clone();
        let height = db.get_block_height()?.unwrap_or_default();
        let id = db.get_block_id(height)?.unwrap_or_default();
        let block = Storage::<fuel_types::Bytes32, FuelBlockDb>::get(&db, &id)?.unwrap_or_default();
        Ok(Block(block.into_owned()))
    }

    async fn base_chain_height(&self) -> U64 {
        0.into()
    }

    async fn peer_count(&self) -> u16 {
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
