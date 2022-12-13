use crate::{
    database::Database,
    executor::Executor,
    service::Config,
};
use fuel_block_producer::ports::DBTransaction;
use fuel_core_interfaces::{
    common::{
        fuel_tx::{
            Receipt,
            Transaction,
        },
        fuel_types::Word,
    },
    executor::{
        Error,
        ExecutionBlock,
        UncommittedResult,
    },
    model::BlockHeight,
    relayer::RelayerDb,
};
#[cfg(feature = "relayer")]
use fuel_relayer::RelayerSynced;
use std::sync::Arc;

pub struct ExecutorAdapter {
    pub database: Database,
    pub config: Config,
}

#[async_trait::async_trait]
impl fuel_block_producer::ports::Executor<Database> for ExecutorAdapter {
    fn execute_without_commit(
        &self,
        block: ExecutionBlock,
    ) -> Result<UncommittedResult<DBTransaction<Database>>, Error> {
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
    ) -> Result<Vec<Vec<Receipt>>, Error> {
        let executor = Executor {
            database: self.database.clone(),
            config: self.config.clone(),
        };
        executor.dry_run(block, utxo_validation)
    }
}

pub struct MaybeRelayerAdapter {
    pub database: Database,
    #[cfg(feature = "relayer")]
    pub relayer_synced: Option<RelayerSynced>,
}

#[async_trait::async_trait]
impl fuel_block_producer::ports::Relayer for MaybeRelayerAdapter {
    async fn get_best_finalized_da_height(
        &self,
    ) -> anyhow::Result<fuel_core_interfaces::model::DaBlockHeight> {
        #[cfg(feature = "relayer")]
        {
            if let Some(sync) = self.relayer_synced.as_ref() {
                sync.await_synced().await?;
            }
        }

        Ok(self
            .database
            .get_finalized_da_height()
            .await
            .unwrap_or_default())
    }
}

pub struct PoACoordinatorAdapter {
    pub block_producer: Arc<fuel_block_producer::Producer<Database>>,
}

#[async_trait::async_trait]
impl fuel_poa_coordinator::ports::BlockProducer<Database> for PoACoordinatorAdapter {
    async fn produce_and_execute_block(
        &self,
        height: BlockHeight,
        max_gas: Word,
    ) -> anyhow::Result<UncommittedResult<DBTransaction<Database>>> {
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
