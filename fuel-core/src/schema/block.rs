use crate::database::{KvStore, KvStoreError, SharedDatabase};
use crate::model::fuel_block::{BlockHeight, FuelBlock};
use crate::schema::scalars::HexString256;
use crate::schema::tx::types::Transaction;
use crate::state::IterDirection;
use async_graphql::connection::Edge;
use async_graphql::connection::{query, Connection, EmptyFields};
use async_graphql::{Context, Object};
use chrono::{DateTime, Utc};
use fuel_tx::Bytes32;
use itertools::Itertools;
use std::convert::TryInto;
use std::ops::Deref;

pub struct Block(pub(crate) FuelBlock);

#[Object]
impl Block {
    async fn id(&self) -> HexString256 {
        HexString256(*self.0.id().deref())
    }

    async fn height(&self) -> u32 {
        self.0.fuel_height.into()
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

    async fn blocks(
        &self,
        ctx: &Context<'_>,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
    ) -> async_graphql::Result<Connection<usize, Block, EmptyFields, EmptyFields>> {
        let db = ctx.data_unchecked::<SharedDatabase>().as_ref();

        query(
            after,
            before,
            first,
            last,
            |after: Option<usize>, before: Option<usize>, first, last| async move {
                let (records_to_fetch, direction) = if let Some(first) = first {
                    (first, IterDirection::Forward)
                } else if let Some(last) = last {
                    (last, IterDirection::Reverse)
                } else {
                    (0, IterDirection::Forward)
                };

                let start;
                let end;

                if direction == IterDirection::Forward {
                    start = after;
                    end = before;
                } else {
                    start = before;
                    end = after;
                }

                let mut blocks = db.all_block_ids(start.map(Into::into), Some(direction));
                let mut started = None;
                if start.is_some() {
                    // skip initial result
                    started = blocks.next();
                }

                // take desired amount of results
                let blocks = blocks
                    .take_while(|r| {
                        if let (Ok(b), Some(end)) = (r, end) {
                            if b.0 == end.into() {
                                return false;
                            }
                        }
                        true
                    })
                    .take(records_to_fetch);
                let mut blocks: Vec<(BlockHeight, Bytes32)> = blocks.try_collect()?;
                if direction == IterDirection::Forward {
                    blocks.reverse();
                }

                // TODO: do a batch get instead
                let blocks: Vec<FuelBlock> = blocks
                    .iter()
                    .map(|(_, id)| {
                        KvStore::<Bytes32, FuelBlock>::get(db, id)
                            .transpose()
                            .ok_or(KvStoreError::NotFound)?
                    })
                    .try_collect()?;

                let mut connection =
                    Connection::new(started.is_some(), records_to_fetch <= blocks.len());
                connection.append(
                    blocks
                        .iter()
                        .map(|item| Edge::new(item.fuel_height.to_usize(), item.clone())),
                );
                Ok(connection)
            },
        )
        .await
        .map(|conn| conn.map_node(Block))
    }
}
