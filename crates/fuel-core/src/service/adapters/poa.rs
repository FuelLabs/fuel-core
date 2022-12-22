use crate::{
    database::Database,
    service::adapters::{
        BlockProducerAdapter,
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
    fn pending_number(&self) -> usize {
        self.service.shared.pending_number()
    }

    fn total_consumable_gas(&self) -> u64 {
        self.service.shared.total_consumable_gas()
    }

    fn remove_txs(&self, ids: Vec<TxId>) -> Vec<ArcPoolTx> {
        self.service.shared.remove_txs(ids)
    }

    async fn next_transaction_status_update(&mut self) -> TxStatus {
        // lazily instantiate a long-lived tx receiver only when there
        // is consumer for the messages to avoid lagging the channel
        if self.tx_status_rx.is_none() {
            self.tx_status_rx = Some(self.service.shared.tx_status_subscribe());
        }

        // TODO: Handle unwrap
        self.tx_status_rx
            .as_mut()
            .expect("Should always be some because we checked that above")
            .recv()
            .await
            .expect("Tx Status Channel errored unexpectedly")
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
