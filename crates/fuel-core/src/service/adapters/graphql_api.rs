use crate::{
    database::Database,
    fuel_core_graphql_api::ports::{
        DatabaseBlocks,
        DatabaseMessages,
        DatabasePort,
        DatabaseTransactions,
    },
    state::IterDirection,
};
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
    },
    not_found,
    Error as StorageError,
    Result as StorageResult,
};
use fuel_core_txpool::types::TxId;
use fuel_core_types::{
    blockchain::primitives::{
        BlockHeight,
        BlockId,
    },
    entities::message::Message,
    fuel_tx::{
        Address,
        MessageId,
    },
    services::txpool::TransactionStatus,
};

impl DatabaseBlocks for Database {
    fn block_id(&self, height: BlockHeight) -> StorageResult<BlockId> {
        self.get_block_id(height)
            .and_then(|heigh| heigh.ok_or(not_found!("BlockId")))
    }

    fn blocks_ids(
        &self,
        start: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<(BlockHeight, BlockId)>> {
        self.all_block_ids(start, direction)
            .map(|result| result.map_err(StorageError::from))
            .into_boxed()
    }

    fn latest_block_ids(&self) -> StorageResult<(BlockHeight, BlockId)> {
        Ok(self
            .latest_block_ids()
            .transpose()
            .ok_or(not_found!("BlockIds"))??)
    }
}

impl DatabaseTransactions for Database {
    fn tx_status(&self, tx_id: &TxId) -> StorageResult<TransactionStatus> {
        Ok(self
            .get_tx_status(tx_id)
            .transpose()
            .ok_or(not_found!("TransactionId"))??)
    }
}

impl DatabaseMessages for Database {
    fn owned_message_ids(
        &self,
        owner: &Address,
        start_message_id: Option<MessageId>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<MessageId>> {
        self.owned_message_ids(owner, start_message_id, Some(direction))
            .map(|result| result.map_err(StorageError::from))
            .into_boxed()
    }

    fn all_messages(
        &self,
        start_message_id: Option<MessageId>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<Message>> {
        self.all_messages(start_message_id, Some(direction))
            .map(|result| result.map_err(StorageError::from))
            .into_boxed()
    }
}

impl DatabasePort for Database {}
