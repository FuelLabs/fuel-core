use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::block::CompressedBlock,
    fuel_tx::{
        Transaction,
        TxId,
    },
    fuel_types::BlockHeight,
};

pub trait L2Data: Send + Sync {
    fn latest_height(&self) -> StorageResult<BlockHeight>;
    fn get_block(&self, height: &BlockHeight) -> StorageResult<Option<CompressedBlock>>;
    fn get_transaction(&self, tx_id: &TxId) -> StorageResult<Option<Transaction>>;
}
