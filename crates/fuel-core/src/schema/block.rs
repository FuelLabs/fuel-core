use super::scalars::{
    Bytes32,
    Tai64Timestamp,
};
use crate::{
    fuel_core_graphql_api::{
        service::ConsensusModule,
        Config as GraphQLConfig,
        IntoApiResult,
    },
    query::{
        BlockQueryContext,
        TransactionQueryContext,
    },
    schema::{
        scalars::{
            BlockId,
            Signature,
            U64,
        },
        tx::types::Transaction,
    },
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
use fuel_core_storage::{
    iter::IntoBoxedIter,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        header::BlockHeader,
    },
    fuel_types,
    tai64::Tai64,
};

pub struct Block(pub(crate) CompressedBlock);

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
        let bytes: fuel_types::Bytes32 = self.0.header().id().into();
        bytes.into()
    }

    async fn header(&self) -> Header {
        self.0.header().clone().into()
    }

    async fn consensus(&self, ctx: &Context<'_>) -> async_graphql::Result<Consensus> {
        let query = BlockQueryContext(ctx.data_unchecked());
        let id = self.0.header().id();
        let consensus = query.consensus(&id)?;

        Ok(consensus.into())
    }

    async fn transactions(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<Transaction>> {
        let query = TransactionQueryContext(ctx.data_unchecked());
        self.0
            .transactions()
            .iter()
            .map(|tx_id| {
                let tx = query.transaction(tx_id)?;
                Ok(tx.into())
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
        let data = BlockQueryContext(ctx.data_unchecked());
        let id = match (id, height) {
            (Some(_), Some(_)) => {
                return Err(async_graphql::Error::new(
                    "Can't provide both an id and a height",
                ))
            }
            (Some(id), None) => Ok(id.0.into()),
            (None, Some(height)) => {
                let height: u64 = height.into();
                let height: u32 = height.try_into()?;
                data.block_id(&height.into())
            }
            (None, None) => {
                return Err(async_graphql::Error::new("Missing either id or height"))
            }
        };

        id.and_then(|id| data.block(&id)).into_api_result()
    }

    async fn blocks(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<Connection<usize, Block, EmptyFields, EmptyFields>> {
        let db = BlockQueryContext(ctx.data_unchecked());
        crate::schema::query_pagination(after, before, first, last, |start, direction| {
            Ok(blocks_query(&db, *start, direction))
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
            .map(|b| b.0.header().clone().into()))
    }

    async fn headers(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<Connection<usize, Header, EmptyFields, EmptyFields>> {
        let db = BlockQueryContext(ctx.data_unchecked());
        crate::schema::query_pagination(after, before, first, last, |start, direction| {
            Ok(blocks_query(&db, *start, direction))
        })
        .await
    }
}

fn blocks_query<'a, T>(
    query: &'a BlockQueryContext<'a>,
    start: Option<usize>,
    direction: IterDirection,
) -> impl Iterator<Item = StorageResult<(usize, T)>> + 'a
where
    T: async_graphql::OutputType,
    T: From<CompressedBlock>,
    T: 'a,
{
    let blocks = query
        .compressed_blocks(start.map(Into::into), direction)
        .map(|result| {
            result.map(|block| (block.header().height().as_usize(), block.into()))
        });

    blocks.into_boxed()
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
        let query = BlockQueryContext(ctx.data_unchecked());
        let consensus_module = ctx.data_unchecked::<ConsensusModule>();
        let config = ctx.data_unchecked::<GraphQLConfig>().clone();

        if !config.manual_blocks_enabled {
            return Err(
                anyhow!("Manual Blocks must be enabled to use this endpoint").into(),
            )
        }

        let latest_block = query.latest_block()?;
        let block_times = get_time_closure(&latest_block, time, blocks_to_produce.0)?;
        consensus_module.manual_produce_block(block_times).await?;

        query
            .latest_block_height()
            .map(Into::into)
            .map_err(Into::into)
    }
}

fn get_time_closure(
    latest_block: &CompressedBlock,
    time_parameters: Option<TimeParameters>,
    blocks_to_produce: u64,
) -> anyhow::Result<Vec<Option<Tai64>>> {
    if let Some(params) = time_parameters {
        let start_time = params.start_time.into();
        check_start_after_latest_block(latest_block, start_time)?;
        check_block_time_overflow(&params, blocks_to_produce)?;
        let interval: u64 = params.block_time_interval.into();

        let vec = (0..blocks_to_produce)
            .into_iter()
            .map(|idx| {
                let (timestamp, _) = params
                    .start_time
                    .0
                    .overflowing_add(interval.overflowing_mul(idx).0);
                Some(Tai64(timestamp))
            })
            .collect();
        return Ok(vec)
    };

    Ok(vec![None; blocks_to_produce as usize])
}

fn check_start_after_latest_block(
    latest_block: &CompressedBlock,
    start_time: u64,
) -> anyhow::Result<()> {
    let current_height = *latest_block.header().height();

    if current_height.as_usize() == 0 {
        return Ok(())
    }

    let latest_time = latest_block.header().time();
    if latest_time.0 > start_time {
        return Err(anyhow!(
            "The start time must be set after the latest block time: {:?}",
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
        Block(block)
    }
}

impl From<BlockHeader> for Header {
    fn from(header: BlockHeader) -> Self {
        Header(header)
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
