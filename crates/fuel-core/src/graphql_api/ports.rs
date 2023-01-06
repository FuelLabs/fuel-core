use crate::state::IterDirection;
use fuel_core_storage::{
    iter::BoxedIter,
    tables::{
        FuelBlocks,
        Messages,
        Receipts,
        SealedBlockConsensus,
        Transactions,
    },
    Error as StorageError,
    Result as StorageResult,
    StorageInspect,
};
use fuel_core_types::{
    blockchain::primitives::{
        BlockHeight,
        BlockId,
    },
    entities::message::Message,
    fuel_tx::TxId,
    fuel_types::{
        Address,
        MessageId,
    },
    services::txpool::TransactionStatus,
};

/// The database port expected by GraphQL API service.
pub trait DatabasePort:
    Send + Sync + DatabaseBlocks + DatabaseTransactions + DatabaseMessages
{
}

/// Trait that specifies all the getters required for blocks.
pub trait DatabaseBlocks:
    StorageInspect<FuelBlocks, Error = StorageError>
    + StorageInspect<SealedBlockConsensus, Error = StorageError>
{
    fn block_id(&self, height: BlockHeight) -> StorageResult<BlockId>;

    fn blocks_ids(
        &self,
        start: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<(BlockHeight, BlockId)>>;

    fn latest_block_ids(&self) -> StorageResult<(BlockHeight, BlockId)>;
}

/// Trait that specifies all the getters required for transactions.
pub trait DatabaseTransactions:
    StorageInspect<Transactions, Error = StorageError>
    + StorageInspect<Receipts, Error = StorageError>
{
    fn tx_status(&self, tx_id: &TxId) -> StorageResult<TransactionStatus>;
}

/// Trait that specifies all the getters required for messages.
pub trait DatabaseMessages: StorageInspect<Messages, Error = StorageError> {
    fn owned_message_ids(
        &self,
        owner: &Address,
        start_message_id: Option<MessageId>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<MessageId>>;

    fn all_messages(
        &self,
        start_message_id: Option<MessageId>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<Message>>;
}
