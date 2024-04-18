use self::import_task::{
    ImportTable,
    ImportTask,
};

use super::{
    progress::MultipleProgressReporter,
    task_manager::TaskManager,
};
mod import_task;
mod off_chain;
mod on_chain;
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
    blockchain::{
        block::Block,
        primitives::DaBlockHeight,
    },
    fuel_types::BlockHeight,
};

pub struct SnapshotImporter {
    db: CombinedDatabase,
    task_manager: TaskManager<()>,
    genesis_block: Block,
    snapshot_reader: SnapshotReader,
    multi_progress_reporter: MultipleProgressReporter,
}

impl SnapshotImporter {
    fn new(
        db: CombinedDatabase,
        genesis_block: Block,
        snapshot_reader: SnapshotReader,
        watcher: StateWatcher,
    ) -> Self {
        Self {
            db,
            genesis_block,
            task_manager: TaskManager::new(watcher),
            snapshot_reader,
            multi_progress_reporter: MultipleProgressReporter::new(tracing::info_span!(
                "snapshot_importer"
            )),
        }
    }

    pub async fn import(
        db: CombinedDatabase,
        genesis_block: Block,
        snapshot_reader: SnapshotReader,
        watcher: StateWatcher,
    ) -> anyhow::Result<()> {
        Self::new(db, genesis_block, snapshot_reader, watcher)
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

    pub fn spawn_worker_on_chain<TableBeingWritten>(&mut self) -> anyhow::Result<()>
    where
        TableBeingWritten: TableWithBlueprint + 'static + Send,
        TableEntry<TableBeingWritten>: serde::de::DeserializeOwned + Send,
        StateConfig: AsTable<TableBeingWritten>,
        Handler<TableBeingWritten>:
            ImportTable<TableInSnapshot = TableBeingWritten, DbDesc = OnChain>,
    {
        let groups = self.snapshot_reader.read::<TableBeingWritten>()?;
        let num_groups = groups.len();

        let block_height = *self.genesis_block.header().height();
        let da_block_height = self.genesis_block.header().da_height;
        let db = self.db.on_chain().clone();

        let progress_reporter = self
            .multi_progress_reporter
            .table_reporter::<TableBeingWritten>(Some(num_groups));

        self.task_manager.spawn(move |token| {
            let task = ImportTask::new(
                token,
                Handler::new(block_height, da_block_height),
                groups,
                db,
                progress_reporter,
            );
            tokio_rayon::spawn(move || task.run())
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
        let block_height = *self.genesis_block.header().height();
        let da_block_height = self.genesis_block.header().da_height;

        let db = self.db.off_chain().clone();

        let progress_reporter = self
            .multi_progress_reporter
            .table_reporter::<TableBeingWritten>(Some(num_groups));

        self.task_manager.spawn(move |token| {
            let task = ImportTask::new(
                token,
                Handler::new(block_height, da_block_height),
                groups,
                db,
                progress_reporter,
            );
            tokio_rayon::spawn(move || task.run())
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
