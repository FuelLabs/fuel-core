use crate::{
    database::{
        storage::FuelBlocks,
        Database,
        KvStoreError,
    },
    executor::Executor,
    model::{
        BlockHeight,
        FuelBlockDb,
    },
    schema::{
        scalars::{
            BlockId,
            U64,
        },
        tx::types::Transaction,
    },
    service::Config,
    state::IterDirection,
};
use anyhow::anyhow;
use async_graphql::{
    connection::{
        query,
        Connection,
        Edge,
        EmptyFields,
    },
    Context,
    InputObject,
    Object,
};
use chrono::{
    DateTime,
    NaiveDateTime,
    Utc,
};
use fuel_core_interfaces::{
    common::{
        fuel_storage::StorageAsRef,
        fuel_types,
    },
    db::Transactions,
    executor::{
        ExecutionBlock,
        Executor as ExecutorTrait,
    },
    model::{
        FuelApplicationHeader,
        FuelConsensusHeader,
        PartialFuelBlock,
        PartialFuelBlockHeader,
    },
};
use itertools::Itertools;
use std::{
    borrow::Cow,
    convert::TryInto,
};

pub struct Block(pub(crate) FuelBlockDb);

#[Object]
impl Block {
    async fn id(&self) -> BlockId {
        self.0.id().into()
    }

    async fn height(&self) -> U64 {
        (*self.0.header.height()).into()
    }

    async fn transactions(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<Transaction>> {
        let db = ctx.data_unchecked::<Database>().clone();
        self.0
            .transactions
            .iter()
            .map(|tx_id| {
                Ok(Transaction(
                    db.storage::<Transactions>()
                        .get(tx_id)
                        .and_then(|v| v.ok_or(KvStoreError::NotFound))?
                        .into_owned(),
                ))
            })
            .collect()
    }

    async fn time(&self) -> DateTime<Utc> {
        *self.0.header.time()
    }
}

#[derive(Default)]
pub struct BlockQuery;

#[Object]
impl BlockQuery {
    async fn block(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "ID of the block")] id: Option<BlockId>,
        #[graphql(desc = "Height of the block")] height: Option<U64>,
    ) -> async_graphql::Result<Option<Block>> {
        let db = ctx.data_unchecked::<Database>();
        let id = match (id, height) {
            (Some(_), Some(_)) => {
                return Err(async_graphql::Error::new(
                    "Can't provide both an id and a height",
                ))
            }
            (Some(id), None) => id.into(),
            (None, Some(height)) => {
                let height: u64 = height.into();
                if height == 0 {
                    return Err(async_graphql::Error::new(
                        "Genesis block isn't implemented yet",
                    ))
                } else {
                    db.get_block_id(height.try_into()?)?
                        .ok_or("Block height non-existent")?
                }
            }
            (None, None) => {
                return Err(async_graphql::Error::new("Missing either id or height"))
            }
        };

        let block = db
            .storage::<FuelBlocks>()
            .get(&id)?
            .map(|b| Block(b.into_owned()));
        Ok(block)
    }

    async fn blocks(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<Connection<usize, Block, EmptyFields, EmptyFields>> {
        let db = ctx.data_unchecked::<Database>().clone();

        query(
            after,
            before,
            first,
            last,
            |after: Option<usize>, before: Option<usize>, first, last| {
                async move {
                    let (records_to_fetch, direction) = if let Some(first) = first {
                        (first, IterDirection::Forward)
                    } else if let Some(last) = last {
                        (last, IterDirection::Reverse)
                    } else {
                        (0, IterDirection::Forward)
                    };

                    if (first.is_some() && before.is_some())
                        || (after.is_some() && before.is_some())
                        || (last.is_some() && after.is_some())
                    {
                        return Err(anyhow!("Wrong argument combination"))
                    }

                    let start;
                    let end;

                    if direction == IterDirection::Forward {
                        start = after;
                        end = before;
                    } else {
                        start = before;
                        end = after;
                    }

                    let mut blocks =
                        db.all_block_ids(start.map(Into::into), Some(direction));
                    let mut started = None;
                    if start.is_some() {
                        // skip initial result
                        started = blocks.next();
                    }

                    // take desired amount of results
                    let blocks = blocks
                        .take_while(|r| {
                            if let (Ok(b), Some(end)) = (r, end) {
                                if b.0.as_usize() == end {
                                    return false
                                }
                            }
                            true
                        })
                        .take(records_to_fetch);
                    let mut blocks: Vec<(BlockHeight, fuel_types::Bytes32)> =
                        blocks.try_collect()?;
                    if direction == IterDirection::Forward {
                        blocks.reverse();
                    }

                    // TODO: do a batch get instead
                    let blocks: Vec<Cow<FuelBlockDb>> = blocks
                        .iter()
                        .map(|(_, id)| {
                            db.storage::<FuelBlocks>()
                                .get(id)
                                .transpose()
                                .ok_or(KvStoreError::NotFound)?
                        })
                        .try_collect()?;

                    let mut connection = Connection::new(
                        started.is_some(),
                        records_to_fetch <= blocks.len(),
                    );

                    connection.edges.extend(blocks.into_iter().map(|item| {
                        Edge::new(
                            item.header.height().to_usize(),
                            Block(item.into_owned()),
                        )
                    }));

                    Ok::<Connection<usize, Block>, anyhow::Error>(connection)
                }
            },
        )
        .await
    }
}

#[derive(InputObject)]
struct TimeParameters {
    /// The time to set on the first block
    start_time: U64,
    /// The time interval between subsequent blocks
    block_time_interval: U64,
}

#[derive(Default)]
pub struct BlockMutation;

#[Object]
impl BlockMutation {
    async fn produce_blocks(
        &self,
        ctx: &Context<'_>,
        blocks_to_produce: U64,
        time: Option<TimeParameters>,
    ) -> async_graphql::Result<U64> {
        let db = ctx.data_unchecked::<Database>();
        let cfg = ctx.data_unchecked::<Config>().clone();

        if !cfg.manual_blocks_enabled {
            return Err(
                anyhow!("Manual Blocks must be enabled to use this endpoint").into(),
            )
        }
        // todo!("trigger block production manually");

        let executor = Executor {
            database: db.clone(),
            config: cfg.clone(),
        };

        let block_time = get_time_closure(db, time, blocks_to_produce.0).await?;

        for idx in 0..blocks_to_produce.0 {
            let current_height = db.get_block_height()?.unwrap_or_default();
            let new_block_height = current_height + 1u32.into();

            let block = PartialFuelBlock::new(
                PartialFuelBlockHeader {
                    consensus: FuelConsensusHeader {
                        height: new_block_height,
                        time: block_time(idx),
                        prev_root: Default::default(),
                        generated: Default::default(),
                    },
                    application: FuelApplicationHeader {
                        da_height: Default::default(),
                        generated: Default::default(),
                    },
                    metadata: Default::default(),
                },
                vec![],
            );

            executor.execute(ExecutionBlock::Production(block)).await?;
        }

        db.get_block_height()?
            .map(|new_height| Ok(new_height.into()))
            .ok_or("Block height not found")?
    }
}

async fn get_time_closure(
    db: &Database,
    time_parameters: Option<TimeParameters>,
    blocks_to_produce: u64,
) -> anyhow::Result<Box<dyn Fn(u64) -> DateTime<Utc> + Send>> {
    if let Some(params) = time_parameters {
        check_start_after_latest_block(db, params.start_time.0).await?;
        check_block_time_overflow(&params, blocks_to_produce).await?;

        return Ok(Box::new(move |idx: u64| {
            let (timestamp, _) = params
                .start_time
                .0
                .overflowing_add(params.block_time_interval.0.overflowing_mul(idx).0);
            let naive = NaiveDateTime::from_timestamp(timestamp as i64, 0);

            DateTime::from_utc(naive, Utc)
        }))
    };

    Ok(Box::new(|_| Utc::now()))
}

async fn check_start_after_latest_block(
    db: &Database,
    start_time: u64,
) -> anyhow::Result<()> {
    let current_height = db.get_block_height()?.unwrap_or_default();

    if current_height.as_usize() == 0 {
        return Ok(())
    }

    let latest_time = db.timestamp(current_height.into())?;
    if latest_time as u64 > start_time {
        return Err(anyhow!(
            "The start time must be set after the latest block time: {}",
            latest_time
        ))
    }

    Ok(())
}

async fn check_block_time_overflow(
    params: &TimeParameters,
    blocks_to_produce: u64,
) -> anyhow::Result<()> {
    let (final_offset, overflow_mul) = params
        .block_time_interval
        .0
        .overflowing_mul(blocks_to_produce);
    let (_, overflow_add) = params.start_time.0.overflowing_add(final_offset);

    if overflow_mul || overflow_add {
        return Err(anyhow!("The provided time parameters lead to an overflow"))
    };

    Ok(())
}

#[cfg(test)]
mod tests {

    #[tokio::test]
    async fn get_time_closure_returns_custom_time() {}

    #[tokio::test]
    async fn get_time_closure_returns_utc_now() {}

    #[tokio::test]
    async fn get_time_closure_bad_start_time_error() {}

    #[tokio::test]
    async fn get_time_closure_overflow_error() {}
}
