use crate::database::{KvStore, KvStoreError, SharedDatabase};
use crate::model::fuel_block::FuelBlock;
use crate::model::Hash;
use async_graphql::connection::{Connection, EmptyFields};
use async_graphql::{Context, Object};
use fuel_tx::Bytes32;
use std::convert::TryInto;
use std::ops::Deref;

pub struct Block(FuelBlock);

#[Object]
impl Block {
    async fn id(&self) -> String {
        hex::encode(self.0.id)
    }

    async fn height(&self) -> u32 {
        self.0.fuel_height
    }

    async fn transactions(&self) -> Vec<String> {
        self.0
            .transactions
            .iter()
            .map(|v| hex::encode(v.deref()))
            .collect()
    }
}

#[derive(Default)]
pub struct BlockQuery;

#[Object]
impl BlockQuery {
    async fn block(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "id of the block")] id: String,
    ) -> async_graphql::Result<Option<Block>> {
        let id = hex::decode(id).map_err(|e| e.to_string())?;
        let id: Bytes32 = id.as_slice().try_into()?;
        let db = ctx.data_unchecked::<SharedDatabase>().0.as_ref().as_ref();
        let block = KvStore::<Bytes32, FuelBlock>::get(db, &id)?.map(|b| Block(b));
        Ok(block)
    }
}
