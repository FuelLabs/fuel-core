use super::scalars::{
    Bytes32,
    Tai64Timestamp,
};
use crate::{
    database::Database,
    executor::Executor,
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
        Connection,
        EmptyFields,
    },
    Context,
    InputObject,
    Object,
    SimpleObject,
    Union,
};
use fuel_core_poa::service::seal_block;
use fuel_core_producer::ports::Executor as ExecutorTrait;
use fuel_core_storage::{
    not_found,
    tables::{
        FuelBlocks,
        SealedBlockConsensus,
        Transactions,
    },
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::{
            CompressedBlock,
            PartialFuelBlock,
        },
        header::{
            ApplicationHeader,
            BlockHeader,
            ConsensusHeader,
            PartialBlockHeader,
        },
    },
    fuel_types,
    services::executor::{
        ExecutionBlock,
        ExecutionResult,
    },
    tai64::Tai64,
};
use itertools::Itertools;
use std::convert::TryInto;

pub struct Block {
    pub(crate) header: Header,
    pub(crate) transactions: Vec<fuel_types::Bytes32>,
}

pub struct Header(pub(crate) BlockHeader);

#[derive(Union)]
pub enum Consensus {
    Genesis(Genesis),
    PoA(PoAConsensus),
}

type CoreGenesis = fuel_core_types::blockchain::consensus::Genesis;
type CoreConsensus = fuel_core_types::blockchain::consensus::Consensus;

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
        let bytes: fuel_core_types::fuel_types::Bytes32 = self.header.0.id().into();
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
        let bytes: fuel_core_types::fuel_types::Bytes32 = self.0.id().into();
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
        let db = ctx.data_unchecked::<Database>();
        crate::schema::query_pagination(after, before, first, last, |start, direction| {
            blocks_query(db, *start, direction)
        })
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
        let db = ctx.data_unchecked::<Database>();
        crate::schema::query_pagination(after, before, first, last, |start, direction| {
            blocks_query(db, *start, direction)
        })
        .await
    }
}

fn blocks_query<T>(
    db: &Database,
    start: Option<usize>,
    direction: IterDirection,
) -> StorageResult<impl Iterator<Item = StorageResult<(usize, T)>> + '_>
where
    T: async_graphql::OutputType,
    T: From<CompressedBlock>,
{
    // TODO: Remove `try_collect`
    let blocks: Vec<_> = db
        .all_block_ids(start.map(Into::into), Some(direction))
        .try_collect()?;
    let blocks = blocks.into_iter().map(move |(height, id)| {
        let value = db
            .storage::<FuelBlocks>()
            .get(&id)
            .transpose()
            .ok_or(not_found!(FuelBlocks))??
            .into_owned();

        Ok((height.to_usize(), value.into()))
    });

    Ok(blocks)
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
                PartialBlockHeader {
                    consensus: ConsensusHeader {
                        height: new_block_height,
                        time: block_time(idx),
                        prev_root: Default::default(),
                        generated: Default::default(),
                    },
                    application: ApplicationHeader {
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
            //  inside CM for `blocks_to_produce` blocks. Also in this case `CM` will notify
            //  `TxPool` about a new block.
            let (ExecutionResult { block, .. }, mut db_transaction) = executor
                .execute_without_commit(ExecutionBlock::Production(block))?
                .into();
            seal_block(&config.consensus_key, &block, db_transaction.as_mut())?;
            db_transaction.commit()?;
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
    if latest_time > start_time {
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

impl From<CompressedBlock> for Block {
    fn from(block: CompressedBlock) -> Self {
        let (header, transactions) = block.into_inner();
        Block {
            header: Header(header),
            transactions,
        }
    }
}

impl From<CompressedBlock> for Header {
    fn from(block: CompressedBlock) -> Self {
        Header(block.into_inner().0)
    }
}

impl From<CoreGenesis> for Genesis {
    fn from(genesis: CoreGenesis) -> Self {
        Genesis {
            chain_config_hash: genesis.chain_config_hash.into(),
            coins_root: genesis.coins_root.into(),
            contracts_root: genesis.contracts_root.into(),
            messages_root: genesis.messages_root.into(),
        }
    }
}

impl From<CoreConsensus> for Consensus {
    fn from(consensus: CoreConsensus) -> Self {
        match consensus {
            CoreConsensus::Genesis(genesis) => Consensus::Genesis(genesis.into()),
            CoreConsensus::PoA(poa) => Consensus::PoA(PoAConsensus {
                signature: poa.signature.into(),
            }),
        }
    }
}
