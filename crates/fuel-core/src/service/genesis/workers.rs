use super::{
    runner::ProcessState,
    GenesisRunner,
};
use std::{
    collections::HashMap,
    marker::PhantomData,
    sync::Arc,
};

use crate::{
    combined_database::CombinedDatabase,
    database::database_description::{
        off_chain::OffChain,
        on_chain::OnChain,
    },
    graphql_api::storage::{
        coins::OwnedCoins,
        contracts::ContractsInfo,
        messages::OwnedMessageIds,
        transactions::{
            OwnedTransactions,
            TransactionStatuses,
        },
    },
};
use fuel_core_chain_config::{
    AsTable,
    SnapshotReader,
    StateConfig,
};
use fuel_core_storage::{
    kv_store::StorageColumn,
    structured_storage::TableWithBlueprint,
    tables::{
        Coins,
        ContractsAssets,
        ContractsLatestUtxo,
        ContractsRawCode,
        ContractsState,
        Messages,
        Transactions,
    },
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::BlockHeight,
};
use tokio::sync::Notify;
use tokio_rayon::AsyncRayonHandle;
use tokio_util::sync::CancellationToken;

pub struct GenesisWorkers {
    db: CombinedDatabase,
    cancel_token: CancellationToken,
    block_height: BlockHeight,
    da_block_height: DaBlockHeight,
    snapshot_reader: SnapshotReader,
    finished_signals: HashMap<String, Arc<Notify>>,
}

impl GenesisWorkers {
    pub fn new(db: CombinedDatabase, snapshot_reader: SnapshotReader) -> Self {
        let block_height = snapshot_reader.block_height();
        let da_block_height = snapshot_reader.da_block_height();
        Self {
            db,
            cancel_token: CancellationToken::new(),
            block_height,
            da_block_height,
            snapshot_reader,
            finished_signals: HashMap::default(),
        }
    }

    pub async fn run_on_chain_imports(&mut self) -> anyhow::Result<()> {
        tracing::info!("Running on-chain imports");
        tokio::try_join!(
            self.spawn_worker_on_chain::<Coins>()?,
            self.spawn_worker_on_chain::<Messages>()?,
            self.spawn_worker_on_chain::<ContractsRawCode>()?,
            self.spawn_worker_on_chain::<ContractsLatestUtxo>()?,
            self.spawn_worker_on_chain::<ContractsState>()?,
            self.spawn_worker_on_chain::<ContractsAssets>()?,
            self.spawn_worker_on_chain::<Transactions>()?,
        )
        .map(|_| ())
    }

    pub async fn run_off_chain_imports(&mut self) -> anyhow::Result<()> {
        tracing::info!("Running off-chain imports");
        // TODO: Should we insert a FuelBlockIdsToHeights entry for the genesis block?
        tokio::try_join!(
            self.spawn_worker_off_chain::<TransactionStatuses, TransactionStatuses>()?,
            self.spawn_worker_off_chain::<OwnedTransactions, OwnedTransactions>()?,
            self.spawn_worker_off_chain::<Messages, OwnedMessageIds>()?,
            self.spawn_worker_off_chain::<Coins, OwnedCoins>()?,
            self.spawn_worker_off_chain::<Transactions, ContractsInfo>()?
        )
        .map(|_| ())
    }

    pub async fn finished(&self) {
        for signal in self.finished_signals.values() {
            signal.notified().await;
        }
    }

    pub fn shutdown(&self) {
        self.cancel_token.cancel()
    }

    pub fn spawn_worker_on_chain<T>(
        &mut self,
    ) -> anyhow::Result<AsyncRayonHandle<anyhow::Result<()>>>
    where
        T: TableWithBlueprint + Send + 'static,
        T::OwnedKey: serde::de::DeserializeOwned + Send,
        T::OwnedValue: serde::de::DeserializeOwned + Send,
        StateConfig: AsTable<T>,
        Handler<T>: ProcessState<TableInSnapshot = T, DbDesc = OnChain>,
    {
        let groups = self.snapshot_reader.read::<T>()?;
        let finished_signal = self.get_signal(T::column().name());

        let runner = GenesisRunner::new(
            Some(finished_signal),
            self.cancel_token.clone(),
            Handler::new(self.block_height, self.da_block_height),
            groups,
            self.db.on_chain().clone(),
        );
        Ok(tokio_rayon::spawn(move || runner.run()))
    }

    // TODO: serde bounds can be written shorter
    pub fn spawn_worker_off_chain<TableInSnapshot, TableBeingWritten>(
        &mut self,
    ) -> anyhow::Result<AsyncRayonHandle<anyhow::Result<()>>>
    where
        TableInSnapshot: TableWithBlueprint + Send + 'static,
        TableInSnapshot::OwnedKey: serde::de::DeserializeOwned + Send,
        TableInSnapshot::OwnedValue: serde::de::DeserializeOwned + Send,
        StateConfig: AsTable<TableInSnapshot>,
        Handler<TableBeingWritten>:
            ProcessState<TableInSnapshot = TableInSnapshot, DbDesc = OffChain>,
        TableBeingWritten: Send + 'static,
    {
        let groups = self.snapshot_reader.read::<TableInSnapshot>()?;
        let finished_signal = self.get_signal(TableInSnapshot::column().name());
        let runner = GenesisRunner::new(
            Some(finished_signal),
            self.cancel_token.clone(),
            Handler::<TableBeingWritten>::new(self.block_height, self.da_block_height),
            groups,
            self.db.off_chain().clone(),
        );
        Ok(tokio_rayon::spawn(move || runner.run()))
    }

    fn get_signal(&mut self, name: &str) -> Arc<Notify> {
        self.finished_signals
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(Notify::new()))
            .clone()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Handler<T> {
    pub block_height: BlockHeight,
    pub da_block_height: DaBlockHeight,
    pub phaton_data: PhantomData<T>,
}

impl<T> Handler<T> {
    pub fn new(block_height: BlockHeight, da_block_height: DaBlockHeight) -> Self {
        Self {
            block_height,
            da_block_height,
            phaton_data: PhantomData,
        }
    }
}
