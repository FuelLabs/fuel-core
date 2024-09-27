use super::{
    BlockImporterAdapter,
    TxPoolAdapter,
};
use crate::database::OnChainIterableKeyValueView;
use fuel_core_p2p::ports::{
    BlockHeightImporter,
    P2pDb,
    TxPool,
};
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::Result as StorageResult;
use fuel_core_txpool::TxId;
use fuel_core_types::{
    blockchain::{
        consensus::Genesis,
        SealedBlockHeader,
    },
    fuel_types::BlockHeight,
    services::p2p::{
        NetworkableTransactionPool,
        Transactions,
    },
};
use std::ops::Range;

impl P2pDb for OnChainIterableKeyValueView {
    fn get_sealed_headers(
        &self,
        block_height_range: Range<u32>,
    ) -> StorageResult<Option<Vec<SealedBlockHeader>>> {
        self.get_sealed_block_headers(block_height_range)
    }

    fn get_transactions(
        &self,
        block_height_range: Range<u32>,
    ) -> StorageResult<Option<Vec<Transactions>>> {
        self.get_transactions_on_blocks(block_height_range)
    }

    fn get_genesis(&self) -> StorageResult<Genesis> {
        self.get_genesis()
    }
}

impl BlockHeightImporter for BlockImporterAdapter {
    fn next_block_height(&self) -> BoxStream<BlockHeight> {
        use tokio_stream::{
            wrappers::BroadcastStream,
            StreamExt,
        };
        Box::pin(
            BroadcastStream::new(self.block_importer.subscribe())
                .filter_map(|result| result.ok())
                .map(|result| *result.sealed_block.entity.header().height()),
        )
    }
}

impl TxPool for TxPoolAdapter {
    fn get_tx_ids(&self, max_txs: usize) -> Vec<TxId> {
        self.service.get_tx_ids(max_txs)
    }

    fn get_full_txs(&self, tx_ids: Vec<TxId>) -> Vec<Option<NetworkableTransactionPool>> {
        self.service
            .find(tx_ids)
            .into_iter()
            .map(|tx_info| {
                tx_info.map(|tx| {
                    NetworkableTransactionPool::PoolTransaction(tx.clone())
                })
            })
            .collect()
    }
}
