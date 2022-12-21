use crate::{
    database::Database,
    executor::Executor,
    service::adapters::{
        ExecutorAdapter,
        MaybeRelayerAdapter,
        TxPoolAdapter,
    },
};
use fuel_core_producer::ports::TxPool;
use fuel_core_storage::{
    transactional::StorageTransaction,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        primitives,
        primitives::BlockHeight,
    },
    fuel_tx::Receipt,
    services::{
        executor::{
            ExecutionBlock,
            Result as ExecutorResult,
            UncommittedResult,
        },
        txpool::{
            ArcPoolTx,
            Error as TxPoolError,
        },
    },
};

#[async_trait::async_trait]
impl TxPool for TxPoolAdapter {
    async fn get_includable_txs(
        &self,
        _block_height: BlockHeight,
        max_gas: u64,
    ) -> Result<Vec<ArcPoolTx>, TxPoolError> {
        self.service.select_transactions(max_gas).await
    }
}

#[async_trait::async_trait]
impl fuel_core_producer::ports::Executor<Database> for ExecutorAdapter {
    fn execute_without_commit(
        &self,
        block: ExecutionBlock,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<Database>>> {
        let executor = Executor {
            database: self.database.clone(),
            config: self.config.clone(),
        };
        executor.execute_without_commit(block)
    }

    fn dry_run(
        &self,
        block: ExecutionBlock,
        utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<Vec<Receipt>>> {
        let executor = Executor {
            database: self.database.clone(),
            config: self.config.clone(),
        };
        executor.dry_run(block, utxo_validation)
    }
}

#[async_trait::async_trait]
impl fuel_core_producer::ports::Relayer for MaybeRelayerAdapter {
    async fn get_best_finalized_da_height(
        &self,
    ) -> StorageResult<primitives::DaBlockHeight> {
        #[cfg(feature = "relayer")]
        {
            use fuel_core_relayer::ports::RelayerDb;
            if let Some(sync) = self.relayer_synced.as_ref() {
                sync.await_synced().await?;
            }

            Ok(self
                .database
                .get_finalized_da_height()
                .await
                .unwrap_or_default())
        }
        #[cfg(not(feature = "relayer"))]
        {
            Ok(Default::default())
        }
    }
}
