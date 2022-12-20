use crate::{
    database::Database,
    service::adapters::{
        PoACoordinatorAdapter,
        TxPoolAdapter,
    },
};
use fuel_core_poa::ports::TransactionPool;
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

#[async_trait::async_trait]
impl TransactionPool for TxPoolAdapter {
    async fn pending_number(&self) -> anyhow::Result<usize> {
        self.service.pending_number().await
    }

    async fn total_consumable_gas(&self) -> anyhow::Result<u64> {
        self.service.total_consumable_gas().await
    }

    async fn remove_txs(&self, ids: Vec<TxId>) -> anyhow::Result<Vec<ArcPoolTx>> {
        self.service.remove_txs(ids).await
    }

    async fn next_transaction_status_update(&mut self) -> TxStatus {
        match self.tx_status_rx.recv().await {
            Ok(status) => return status,
            Err(err) => {
                panic!("Tx Status Channel errored unexpectedly: {err:?}");
            }
        }
    }
}

#[async_trait::async_trait]
impl fuel_core_poa::ports::BlockProducer<Database> for PoACoordinatorAdapter {
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
