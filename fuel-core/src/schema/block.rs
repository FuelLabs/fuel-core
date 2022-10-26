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
    Object,
};
use chrono::{
    DateTime,
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
        FuelBlockHeader,
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

use super::scalars::Bytes32;

pub struct Block {
    pub(crate) header: Header,
    pub(crate) transactions: Vec<fuel_types::Bytes32>,
}

pub struct Header(pub(crate) FuelBlockHeader);

#[Object]
impl Block {
    async fn id(&self) -> BlockId {
        let bytes: fuel_core_interfaces::common::prelude::Bytes32 =
            self.header.0.id().into();
        bytes.into()
    }

    async fn header(&self) -> &Header {
        &self.header
    }

    async fn transactions(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<Transaction>> {
        let db = ctx.data_unchecked::<Database>().clone();
        self.transactions
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
}

#[Object]
impl Header {
    /// Hash of the header
    async fn id(&self) -> BlockId {
        let bytes: fuel_core_interfaces::common::prelude::Bytes32 = self.0.id().into();
        bytes.into()
    }

    /// The layer 1 height of messages and events to include since the last layer 1 block number.
    async fn da_height(&self) -> U64 {
        self.0.da_height.0.into()
    }

    /// Number of transactions in this block.
    async fn transactions_count(&self) -> U64 {
        self.0.transactions_count.into()
    }

    /// Number of output messages in this block.
    async fn output_messages_count(&self) -> U64 {
        self.0.output_messages_count.into()
    }

    /// Merkle root of transactions.
    async fn transactions_root(&self) -> Bytes32 {
        self.0.transactions_root.into()
    }

    /// Merkle root of messages in this block.
    async fn output_messages_root(&self) -> Bytes32 {
        self.0.output_messages_root.into()
    }

    /// Fuel block height.
    async fn height(&self) -> U64 {
        (*self.0.height()).into()
    }

    /// Merkle root of all previous block header hashes.
    async fn prev_root(&self) -> Bytes32 {
        (*self.0.prev_root()).into()
    }

    /// The block producer time.
    async fn time(&self) -> DateTime<Utc> {
        *self.0.time()
    }

    /// Hash of the application header.
    async fn application_hash(&self) -> Bytes32 {
        (*self.0.application_hash()).into()
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
            .map(|b| Block::from(b.into_owned()));
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
            |after: Option<usize>, before: Option<usize>, first, last| async move {
                blocks_query(db, first, after, last, before)
            },
        )
        .await
    }
}

#[derive(Default)]
pub struct HeaderQuery;

#[Object]
impl HeaderQuery {
    async fn header(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "ID of the block")] id: Option<BlockId>,
        #[graphql(desc = "Height of the block")] height: Option<U64>,
    ) -> async_graphql::Result<Option<Header>> {
        Ok(BlockQuery {}
            .block(ctx, id, height)
            .await?
            .map(|b| b.header))
    }

    async fn headers(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<Connection<usize, Header, EmptyFields, EmptyFields>> {
        let db = ctx.data_unchecked::<Database>().clone();

        query(
            after,
            before,
            first,
            last,
            |after: Option<usize>, before: Option<usize>, first, last| async move {
                blocks_query(db, first, after, last, before)
            },
        )
        .await
    }
}

fn blocks_query<T>(
    db: Database,
    first: Option<usize>,
    after: Option<usize>,
    last: Option<usize>,
    before: Option<usize>,
) -> anyhow::Result<Connection<usize, T, EmptyFields, EmptyFields>>
where
    T: async_graphql::OutputType,
    T: From<FuelBlockDb>,
{
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
                if b.0.as_usize() == end {
                    return false
                }
            }
            true
        })
        .take(records_to_fetch);
    let mut blocks: Vec<(BlockHeight, fuel_types::Bytes32)> = blocks.try_collect()?;
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

    let mut connection =
        Connection::new(started.is_some(), records_to_fetch <= blocks.len());

    connection.edges.extend(blocks.into_iter().map(|item| {
        Edge::new(item.header.height().to_usize(), T::from(item.into_owned()))
    }));

    Ok::<Connection<usize, T>, anyhow::Error>(connection)
}
#[derive(Default)]
pub struct BlockMutation;

#[Object]
impl BlockMutation {
    async fn produce_blocks(
        &self,
        ctx: &Context<'_>,
        blocks_to_produce: U64,
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

        for _ in 0..blocks_to_produce.0 {
            let current_height = db.get_block_height()?.unwrap_or_default();
            let new_block_height = current_height + 1u32.into();

            let block = PartialFuelBlock::new(
                PartialFuelBlockHeader {
                    consensus: FuelConsensusHeader {
                        height: new_block_height,
                        time: Utc::now(),
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

impl From<FuelBlockDb> for Block {
    fn from(block: FuelBlockDb) -> Self {
        let FuelBlockDb {
            header,
            transactions,
        } = block;
        Block {
            header: Header(header),
            transactions,
        }
    }
}

impl From<FuelBlockDb> for Header {
    fn from(block: FuelBlockDb) -> Self {
        Header(block.header)
    }
}
