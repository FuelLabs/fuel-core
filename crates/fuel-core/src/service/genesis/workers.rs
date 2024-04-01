use super::{runner::ProcessState, GenesisRunner};
use std::marker::PhantomData;

use crate::{
    combined_database::CombinedDatabase,
    database::database_description::{off_chain::OffChain, on_chain::OnChain},
    graphql_api::storage::{
        coins::OwnedCoins,
        contracts::ContractsInfo,
        messages::OwnedMessageIds,
        transactions::{OwnedTransactions, TransactionStatuses},
    },
};
use fuel_core_chain_config::{AsTable, SnapshotReader, StateConfig, TaskManager};
use fuel_core_storage::{
    kv_store::StorageColumn,
    structured_storage::TableWithBlueprint,
    tables::{
        Coins, ContractsAssets, ContractsLatestUtxo, ContractsRawCode, ContractsState,
        Messages, Transactions,
    },
};
use fuel_core_types::{blockchain::primitives::DaBlockHeight, fuel_types::BlockHeight};
use tokio::sync::Notify;
use tokio_rayon::AsyncRayonHandle;
use tokio_util::sync::CancellationToken;

pub struct SnapshotImporter {
    db: CombinedDatabase,
    task_manager: TaskManager<()>,
    cancel_token: CancellationToken,
    snapshot_reader: SnapshotReader,
}

impl SnapshotImporter {
    fn new(
        db: CombinedDatabase,
        snapshot_reader: SnapshotReader,
        cancel_token: CancellationToken,
    ) -> Self {
        Self {
            db,
            task_manager: TaskManager::new(cancel_token.clone()),
            snapshot_reader,
            cancel_token,
        }
    }

    pub async fn import(
        db: CombinedDatabase,
        snapshot_reader: SnapshotReader,
        cancel_token: CancellationToken,
    ) -> anyhow::Result<()> {
        Self::new(db, snapshot_reader, cancel_token)
            .run_workers()
            .await
    }

    async fn run_workers(mut self) -> anyhow::Result<()> {
        tracing::info!("Running imports");
        self.spawn_worker_on_chain::<Coins>()?;
        self.spawn_worker_on_chain::<Messages>()?;
        self.spawn_worker_on_chain::<ContractsRawCode>()?;
        self.spawn_worker_on_chain::<ContractsLatestUtxo>()?;
        self.spawn_worker_on_chain::<ContractsState>()?;
        self.spawn_worker_on_chain::<ContractsAssets>()?;
        self.spawn_worker_on_chain::<Transactions>()?;

        self.spawn_worker_off_chain::<TransactionStatuses, TransactionStatuses>()?;
        self.spawn_worker_off_chain::<OwnedTransactions, OwnedTransactions>()?;
        self.spawn_worker_off_chain::<Messages, OwnedMessageIds>()?;
        self.spawn_worker_off_chain::<Coins, OwnedCoins>()?;
        self.spawn_worker_off_chain::<Transactions, ContractsInfo>()?;

        self.task_manager.wait().await?;
        Ok(())
    }

    pub fn shutdown(&self) {
        self.cancel_token.cancel();
    }

    pub fn spawn_worker_on_chain<T>(&mut self) -> anyhow::Result<()>
    where
        T: TableWithBlueprint + Send + 'static,
        T::OwnedKey: serde::de::DeserializeOwned + Send,
        T::OwnedValue: serde::de::DeserializeOwned + Send,
        StateConfig: AsTable<T>,
        Handler<T>: ProcessState<TableInSnapshot = T, DbDesc = OnChain>,
    {
        let groups = self.snapshot_reader.read::<T>()?;

        let block_height = self.snapshot_reader.block_height();
        let da_block_height = self.snapshot_reader.da_block_height();
        let db = self.db.on_chain().clone();
        self.task_manager.spawn(move |token| async move {
            GenesisRunner::new(
                token,
                Handler::new(block_height, da_block_height),
                groups,
                db,
            )
            .run()
        });

        Ok(())
    }

    // TODO: serde bounds can be written shorter
    pub fn spawn_worker_off_chain<TableInSnapshot, TableBeingWritten>(
        &mut self,
    ) -> anyhow::Result<()>
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
        let block_height = self.snapshot_reader.block_height();
        let da_block_height = self.snapshot_reader.da_block_height();

        let db = self.db.off_chain().clone();
        self.task_manager.spawn(move |token| async move {
            let runner = GenesisRunner::new(
                token,
                Handler::<TableBeingWritten>::new(block_height, da_block_height),
                groups,
                db,
            );
            runner.run()
        });

        Ok(())
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
