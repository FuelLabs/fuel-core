use self::{
    import_task::{
        ImportTable,
        ImportTask,
    },
    progress::{
        MultipleProgressReporter,
        ProgressReporter,
    },
};

use super::task_manager::TaskManager;
mod import_task;
mod off_chain;
mod on_chain;
mod progress;
use std::marker::PhantomData;

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
    TableEntry,
};
use fuel_core_services::StateWatcher;
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

use tracing::{
    Level,
    Span,
};

pub struct SnapshotImporter {
    db: CombinedDatabase,
    task_manager: TaskManager<bool>,
    snapshot_reader: SnapshotReader,
    tracing_span: tracing::Span,
    multi_progress_reporter: MultipleProgressReporter,
}

impl SnapshotImporter {
    fn new(
        db: CombinedDatabase,
        snapshot_reader: SnapshotReader,
        watcher: StateWatcher,
    ) -> Self {
        Self {
            db,
            task_manager: TaskManager::new(watcher),
            snapshot_reader,
            tracing_span: tracing::info_span!("snapshot_importer"),
            multi_progress_reporter: MultipleProgressReporter::new(),
        }
    }

    pub async fn import(
        db: CombinedDatabase,
        snapshot_reader: SnapshotReader,
        watcher: StateWatcher,
    ) -> anyhow::Result<bool> {
        Self::new(db, snapshot_reader, watcher).run_workers().await
    }

    async fn run_workers(mut self) -> anyhow::Result<bool> {
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

        Ok(self.task_manager.wait().await?.into_iter().all(|e| e))
    }

    pub fn spawn_worker_on_chain<T>(&mut self) -> anyhow::Result<()>
    where
        T: TableWithBlueprint + 'static,
        TableEntry<T>: serde::de::DeserializeOwned + Send,
        StateConfig: AsTable<T>,
        Handler<T>: ImportTable<TableInSnapshot = T, DbDesc = OnChain>,
    {
        let groups = self.snapshot_reader.read::<T>()?;
        let num_groups = groups.len();

        let block_height = self.snapshot_reader.block_height();
        let da_block_height = self.snapshot_reader.da_block_height();
        let db = self.db.on_chain().clone();

        let span = self.create_tracing_span::<T>();
        let progress_reporter = self.progress_reporter(num_groups);

        self.task_manager.spawn(move |token| {
            tokio_rayon::spawn(move || {
                let task = ImportTask::new(
                    token,
                    Handler::new(block_height, da_block_height),
                    groups,
                    db,
                    progress_reporter,
                );
                span.in_scope(|| task.run())
            })
        });

        Ok(())
    }

    pub fn spawn_worker_off_chain<TableInSnapshot, TableBeingWritten>(
        &mut self,
    ) -> anyhow::Result<()>
    where
        TableInSnapshot: TableWithBlueprint + 'static,
        TableEntry<TableInSnapshot>: serde::de::DeserializeOwned + Send,
        StateConfig: AsTable<TableInSnapshot>,
        Handler<TableBeingWritten>:
            ImportTable<TableInSnapshot = TableInSnapshot, DbDesc = OffChain>,
        TableBeingWritten: TableWithBlueprint + Send + 'static,
    {
        let groups = self.snapshot_reader.read::<TableInSnapshot>()?;
        let num_groups = groups.len();
        let block_height = self.snapshot_reader.block_height();
        let da_block_height = self.snapshot_reader.da_block_height();

        let db = self.db.off_chain().clone();
        let span = self.create_tracing_span::<TableBeingWritten>();

        let progress_reporter = self.progress_reporter(num_groups);

        self.task_manager.spawn(move |token| {
            tokio_rayon::spawn(move || {
                let runner = ImportTask::new(
                    token,
                    Handler::<TableBeingWritten>::new(block_height, da_block_height),
                    groups,
                    db,
                    progress_reporter,
                );

                span.in_scope(|| runner.run())
            })
        });

        Ok(())
    }

    fn progress_reporter(&self, num_groups: usize) -> ProgressReporter {
        self.multi_progress_reporter
            .register(ProgressReporter::new_detect_output(num_groups))
    }

    fn create_tracing_span<T: TableWithBlueprint>(&self) -> Span {
        tracing::span!(
            parent: &self.tracing_span,
            Level::INFO,
            "task",
            table = T::column().name()
        )
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
