use crate::database::{KvStore, SharedDatabase};
use crate::model::fuel_block::{BlockHeight, FuelBlock};
use crate::schema::scalars::HexString256;
use crate::schema::tx::types::Transaction;
use async_graphql::{Context, Object};
use chrono::{DateTime, Utc};
use fuel_tx::Bytes32;
use std::convert::TryInto;
use std::ops::Deref;

pub struct Block(pub(crate) FuelBlock);

#[Object]
impl Block {
    async fn id(&self) -> HexString256 {
        HexString256(*self.0.id().deref())
    }

    async fn height(&self) -> u32 {
        self.0.fuel_height
    }

    async fn transactions(&self, ctx: &Context<'_>) -> Vec<Transaction> {
        let db = ctx.data_unchecked::<SharedDatabase>().as_ref();
        self.0
            .transactions
            .iter()
            .map(|tx_id| {
                Transaction(
                    KvStore::<Bytes32, fuel_tx::Transaction>::get(db, tx_id)
                        // TODO: error handling
                        .unwrap()
                        .unwrap(),
                )
            })
            .collect()
    }

    async fn time(&self) -> DateTime<Utc> {
        self.0.time
    }
}

#[derive(Default)]
pub struct BlockQuery;

#[Object]
impl BlockQuery {
    async fn block(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "id of the block")] id: HexString256,
    ) -> async_graphql::Result<Option<Block>> {
        let id: Bytes32 = id.0.try_into()?;
        let db = ctx.data_unchecked::<SharedDatabase>().as_ref();
        let block = KvStore::<Bytes32, FuelBlock>::get(db, &id)?.map(|b| Block(b));
        Ok(block)
    }
}
