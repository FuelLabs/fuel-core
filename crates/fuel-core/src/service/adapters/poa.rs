use crate::{
    database::Database,
    service::adapters::{
        BlockProducerAdapter,
        TxPoolAdapter,
    },
};
use fuel_core_poa::ports::TransactionPool;
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::transactional::StorageTransaction;
use fuel_core_types::{
    blockchain::primitives::BlockHeight,
    fuel_asm::Word,
    fuel_tx::{
        Receipt,
        Transaction,
        TxId,
    },
    services::{
        executor::UncommittedResult,
        txpool::{
            ArcPoolTx,
            TxStatus,
        },
    },
};

impl TransactionPool for TxPoolAdapter {
    fn pending_number(&self) -> usize {
        self.service.pending_number()
    }

    fn total_consumable_gas(&self) -> u64 {
        self.service.total_consumable_gas()
    }

    fn remove_txs(&self, ids: Vec<TxId>) -> Vec<ArcPoolTx> {
        self.service.remove_txs(ids)
    }

    fn transaction_status_events(&self) -> BoxStream<TxStatus> {
        use tokio_stream::{
            wrappers::BroadcastStream,
            StreamExt,
        };
        Box::pin(
            BroadcastStream::new(self.service.tx_status_subscribe())
                .filter_map(|result| result.ok()),
        )
    }
}

#[async_trait::async_trait]
impl fuel_core_poa::ports::BlockProducer<Database> for BlockProducerAdapter {
    async fn produce_and_execute_block(
        &self,
        height: BlockHeight,
        max_gas: Word,
    ) -> anyhow::Result<UncommittedResult<StorageTransaction<Database>>> {
        self.block_producer
            .produce_and_execute_block(height, max_gas)
            .await
    }

    async fn dry_run(
        &self,
        transaction: Transaction,
        height: Option<BlockHeight>,
        utxo_validation: Option<bool>,
    ) -> anyhow::Result<Vec<Receipt>> {
        self.block_producer
            .dry_run(transaction, height, utxo_validation)
            .await
    }
}
