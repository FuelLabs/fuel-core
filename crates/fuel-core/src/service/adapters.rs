use crate::{
    database::Database,
    executor::Executor,
    service::Config,
};
use fuel_core_interfaces::{
    p2p::TransactionGossipData,
    relayer::RelayerDb,
};
use fuel_core_types::{
    blockchain::{
        primitives::BlockId,
        SealedBlock,
    },
    fuel_tx::{
        Receipt,
        Transaction,
    },
    fuel_types::Word,
};

#[cfg(feature = "relayer")]
use fuel_core_relayer::RelayerSynced;
use fuel_core_storage::{
    transactional::StorageTransaction,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        primitives,
        primitives::BlockHeight,
    },
    services::executor::{
        ExecutionBlock,
        Result as ExecutorResult,
        UncommittedResult,
    },
};
use std::sync::Arc;

#[cfg(feature = "p2p")]
use fuel_core_p2p::{
    orchestrator::Service as P2pService,
    ports::Database as P2pDb,
};

pub struct ExecutorAdapter {
    pub database: Database,
    pub config: Config,
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

pub struct MaybeRelayerAdapter {
    pub database: Database,
    #[cfg(feature = "relayer")]
    pub relayer_synced: Option<RelayerSynced>,
}

#[async_trait::async_trait]
impl fuel_core_producer::ports::Relayer for MaybeRelayerAdapter {
    async fn get_best_finalized_da_height(
        &self,
    ) -> StorageResult<primitives::DaBlockHeight> {
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
    pub block_producer: Arc<fuel_core_producer::Producer<Database>>,
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

#[cfg(feature = "p2p")]
pub struct P2pAdapter {
    p2p_service: P2pService,
}

impl P2pAdapter {
    pub fn new(p2p_service: P2pService) -> Self {
        Self { p2p_service }
    }
}

#[async_trait::async_trait]
impl fuel_core_txpool::ports::PeerToPeer for P2pAdapter {
    type GossipedTransaction = TransactionGossipData;

    async fn broadcast_transaction(
        &self,
        transaction: Arc<Transaction>,
    ) -> anyhow::Result<()> {
        self.p2p_service.broadcast_transaction(transaction).await
    }

    async fn next_gossiped_transaction(&self) -> Self::GossipedTransaction {
        todo!()
    }

    fn notify_gossip_transaction_validity(
        &self,
        message: &Self::GossipedTransaction,
        validity: fuel_core_txpool::ports::GossipValidity,
    ) {
        todo!()
    }
}

pub struct DbAdapter {
    database: Database,
}

impl DbAdapter {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

#[async_trait::async_trait]
impl P2pDb for DbAdapter {
    async fn get_sealed_block(&self, block_id: BlockId) -> Option<Arc<SealedBlock>> {
        // todo: current signatures do not match?
        todo!()
    }
}
