use super::scalars::{
    Bytes32,
    Tai64Timestamp,
};
use crate::{
    fuel_core_graphql_api::{
        service::{
            ConsensusModule,
            Database,
        },
        Config as GraphQLConfig,
    },
    graphql_api::IntoApiResult,
    query::{
        BlockQueryData,
        SimpleBlockData,
        SimpleTransactionData,
    },
    schema::{
        scalars::{
            BlockId,
            Signature,
            U32,
            U64,
        },
        tx::types::Transaction,
    },
};
use anyhow::anyhow;
use async_graphql::{
    connection::{
        Connection,
        EmptyFields,
    },
    Context,
    Object,
    SimpleObject,
    Union,
};
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
    },
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        header::BlockHeader,
    },
    fuel_types,
    fuel_types::BlockHeight,
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
        let query: &Database = ctx.data_unchecked();
        let id = self.0.header().id();
        let consensus = query.consensus(&id)?;

        Ok(consensus.into())
    }

    async fn transactions(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<Transaction>> {
        let query: &Database = ctx.data_unchecked();
        self.0
            .transactions()
            .iter()
            .map(|tx_id| {
                let tx = query.transaction(tx_id)?;
                Ok(Transaction::from_tx(*tx_id, tx))
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

    /// Number of message receipts in this block.
    async fn message_receipt_count(&self) -> U64 {
        self.0.message_receipt_count.into()
    }

    /// Merkle root of transactions.
    async fn transactions_root(&self) -> Bytes32 {
        self.0.transactions_root.into()
    }

    /// Merkle root of message receipts in this block.
    async fn message_receipt_root(&self) -> Bytes32 {
        self.0.message_receipt_root.into()
    }

    /// Fuel block height.
    async fn height(&self) -> U32 {
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
        #[graphql(desc = "Height of the block")] height: Option<U32>,
    ) -> async_graphql::Result<Option<Block>> {
        let data: &Database = ctx.data_unchecked();
        let id = match (id, height) {
            (Some(_), Some(_)) => {
                return Err(async_graphql::Error::new(
                    "Can't provide both an id and a height",
                ))
            }
            (Some(id), None) => Ok(id.0.into()),
            (None, Some(height)) => {
                let height: u32 = height.into();
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
    ) -> async_graphql::Result<Connection<U32, Block, EmptyFields, EmptyFields>> {
        let db: &Database = ctx.data_unchecked();
        crate::schema::query_pagination(after, before, first, last, |start, direction| {
            Ok(blocks_query(db, start.map(Into::into), direction))
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
        #[graphql(desc = "Height of the block")] height: Option<U32>,
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
    ) -> async_graphql::Result<Connection<U32, Header, EmptyFields, EmptyFields>> {
        let db: &Database = ctx.data_unchecked();
        crate::schema::query_pagination(after, before, first, last, |start, direction| {
            Ok(blocks_query(db, start.map(Into::into), direction))
        })
        .await
    }
}

fn blocks_query<T>(
    query: &Database,
    start: Option<BlockHeight>,
    direction: IterDirection,
) -> BoxedIter<StorageResult<(U32, T)>>
where
    T: async_graphql::OutputType,
    T: From<CompressedBlock>,
{
    let blocks = query.compressed_blocks(start, direction).map(|result| {
        result.map(|block| ((*block.header().height()).into(), block.into()))
    });

    blocks.into_boxed()
}

#[derive(Default)]
pub struct BlockMutation;

#[Object]
impl BlockMutation {
    /// Sequentially produces `blocks_to_produce` blocks. The first block starts with
    /// `start_timestamp`. If the block production in the [`crate::service::Config`] is
    /// `Trigger::Interval { block_time }`, produces blocks with `block_time ` intervals between
    /// them. The `start_timestamp` is the timestamp in seconds.
    async fn produce_blocks(
        &self,
        ctx: &Context<'_>,
        start_timestamp: Option<Tai64Timestamp>,
        blocks_to_produce: U32,
    ) -> async_graphql::Result<U32> {
        let query: &Database = ctx.data_unchecked();
        let consensus_module = ctx.data_unchecked::<ConsensusModule>();
        let config = ctx.data_unchecked::<GraphQLConfig>().clone();

        if !config.debug {
            return Err(anyhow!("`debug` must be enabled to use this endpoint").into())
        }

        let start_time = start_timestamp.map(|timestamp| timestamp.0);
        let blocks_to_produce: u32 = blocks_to_produce.into();
        consensus_module
            .manually_produce_blocks(start_time, blocks_to_produce)
            .await?;

        query
            .latest_block_height()
            .map(Into::into)
            .map_err(Into::into)
    }
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
