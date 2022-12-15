use super::scalars::{
    Bytes32,
    Tai64Timestamp,
};
use crate::{
    database::Database,
    executor::Executor,
    model::{
        BlockHeight,
        FuelBlockDb,
    },
    schema::{
        scalars::{
            BlockId,
            Signature,
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
    SimpleObject,
    Union,
};
use fuel_block_producer::ports::Executor as ExecutorTrait;
use fuel_core_interfaces::{
    common::{
        fuel_storage::StorageAsRef,
        fuel_types,
        tai64::Tai64,
    },
    db::{
        FuelBlocks,
        SealedBlockConsensus,
        Transactions,
    },
    executor::{
        ExecutionBlock,
        ExecutionResult,
    },
    model::{
        FuelApplicationHeader,
        FuelBlockConsensus,
        FuelBlockHeader,
        FuelConsensusHeader,
        Genesis as FuelGenesis,
        PartialFuelBlock,
        PartialFuelBlockHeader,
    },
    not_found,
};
use fuel_poa_coordinator::service::seal_block;
use itertools::Itertools;
use std::{
    borrow::Cow,
    convert::TryInto,
};

pub struct Block {
    pub(crate) header: Header,
    pub(crate) transactions: Vec<fuel_types::Bytes32>,
}

pub struct Header(pub(crate) FuelBlockHeader);

#[derive(Union)]
pub enum Consensus {
    Genesis(Genesis),
    PoA(PoAConsensus),
}

#[derive(SimpleObject)]
pub struct Genesis {
    /// The chain configs define what consensus type to use, what settlement layer to use,
    /// rules of block validity, etc.
    pub chain_config_hash: Bytes32,
    /// The Binary Merkle Tree root of all genesis coins.
    pub coins_root: Bytes32,
    /// The Binary Merkle Tree root of state, balances, contracts code hash of each contract.
    pub contracts_root: Bytes32,
    /// The Binary Merkle Tree root of all genesis messages.
    pub messages_root: Bytes32,
}

pub struct PoAConsensus {
    signature: Signature,
}

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

    async fn consensus(&self, ctx: &Context<'_>) -> async_graphql::Result<Consensus> {
        let db = ctx.data_unchecked::<Database>().clone();
        let id = self.header.0.id().into();
        let consensus = db
            .storage::<SealedBlockConsensus>()
            .get(&id)
            .map(|c| c.map(|c| c.into_owned().into()))?
            .ok_or(not_found!(SealedBlockConsensus))?;

        Ok(consensus)
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
                        .and_then(|v| v.ok_or(not_found!(Transactions)))?
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
    async fn time(&self) -> Tai64Timestamp {
        Tai64Timestamp(self.0.time())
    }

    /// Hash of the application header.
    async fn application_hash(&self) -> Bytes32 {
        (*self.0.application_hash()).into()
    }
}

#[Object]
impl PoAConsensus {
    /// Gets the signature of the block produced by `PoA` consensus.
    async fn signature(&self) -> Signature {
        self.signature
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
                db.get_block_id(height.try_into()?)?
                    .ok_or("Block height non-existent")?
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
    let blocks: Vec<(BlockHeight, fuel_types::Bytes32)> = blocks.try_collect()?;

    // TODO: do a batch get instead
    let blocks: Vec<Cow<FuelBlockDb>> = blocks
        .iter()
        .map(|(_, id)| {
            db.storage::<FuelBlocks>()
                .get(id)
                .transpose()
                .ok_or(not_found!(FuelBlocks))?
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

#[derive(InputObject)]
struct TimeParameters {
    /// The time to set on the first block
    start_time: U64,
    /// The time interval between subsequent blocks
    block_time_interval: U64,
}

#[Object]
impl BlockMutation {
    async fn produce_blocks(
        &self,
        ctx: &Context<'_>,
        blocks_to_produce: U64,
        time: Option<TimeParameters>,
    ) -> async_graphql::Result<U64> {
        let db = ctx.data_unchecked::<Database>();
        let config = ctx.data_unchecked::<Config>().clone();

        if !config.manual_blocks_enabled {
            return Err(
                anyhow!("Manual Blocks must be enabled to use this endpoint").into(),
            )
        }
        // todo!("trigger block production manually");

        let executor = Executor {
            database: db.clone(),
            config: config.clone(),
        };

        let block_time = get_time_closure(db, time, blocks_to_produce.0)?;

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

            // TODO: Instead of using raw `Executor` here, we need to use `CM` - Consensus Module.
            //  It will guarantee that the block is entirely valid and all information is stored.
            //  For that, we need to manually trigger block production and reset/ignore timers
            //  inside CM for `blocks_to_produce` blocks.
            let (ExecutionResult { block, .. }, mut db_transaction) = executor
                .execute_without_commit(ExecutionBlock::Production(block))?
                .into();
            seal_block(&config.consensus_key, &block, db_transaction.database_mut())?;
            db_transaction.commit_box()?;
        }

        db.get_block_height()?
            .map(|new_height| Ok(new_height.into()))
            .ok_or("Block height not found")?
    }
}

fn get_time_closure(
    db: &Database,
    time_parameters: Option<TimeParameters>,
    blocks_to_produce: u64,
) -> anyhow::Result<Box<dyn Fn(u64) -> Tai64 + Send>> {
    if let Some(params) = time_parameters {
        check_start_after_latest_block(db, params.start_time.0)?;
        check_block_time_overflow(&params, blocks_to_produce)?;

        return Ok(Box::new(move |idx: u64| {
            let (timestamp, _) = params
                .start_time
                .0
                .overflowing_add(params.block_time_interval.0.overflowing_mul(idx).0);
            Tai64(timestamp)
        }))
    };

    Ok(Box::new(|_| Tai64::now()))
}

fn check_start_after_latest_block(db: &Database, start_time: u64) -> anyhow::Result<()> {
    let current_height = db.get_block_height()?.unwrap_or_default();

    if current_height.as_usize() == 0 {
        return Ok(())
    }

    let latest_time = db.block_time(current_height.into())?.0;
    if latest_time as u64 > start_time {
        return Err(anyhow!(
            "The start time must be set after the latest block time: {}",
            latest_time
        ))
    }

    Ok(())
}

fn check_block_time_overflow(
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

impl From<FuelGenesis> for Genesis {
    fn from(genesis: FuelGenesis) -> Self {
        Genesis {
            chain_config_hash: genesis.chain_config_hash.into(),
            coins_root: genesis.coins_root.into(),
            contracts_root: genesis.contracts_root.into(),
            messages_root: genesis.messages_root.into(),
        }
    }
}

impl From<FuelBlockConsensus> for Consensus {
    fn from(consensus: FuelBlockConsensus) -> Self {
        match consensus {
            FuelBlockConsensus::Genesis(genesis) => Consensus::Genesis(genesis.into()),
            FuelBlockConsensus::PoA(poa) => Consensus::PoA(PoAConsensus {
                signature: poa.signature.into(),
            }),
        }
    }
}
