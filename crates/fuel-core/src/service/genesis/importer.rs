use self::{
    import_task::ImportTable,
    progress::{
        MultipleProgressReporter,
        ProgressReporter,
        Target,
    },
};

use super::task_manager::TaskManager;
mod import_task;
mod on_chain;
mod progress;
use std::{
    io::IsTerminal,
    marker::PhantomData,
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

use tracing::Level;

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
            multi_progress_reporter: Self::init_multi_progress_reporter(),
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
        self.spawn_worker::<Coins>()?;
        self.spawn_worker::<Messages>()?;
        self.spawn_worker::<ContractsRawCode>()?;
        self.spawn_worker::<ContractsLatestUtxo>()?;
        self.spawn_worker::<ContractsState>()?;
        self.spawn_worker::<ContractsAssets>()?;
        self.spawn_worker::<Transactions>()?;
        self.spawn_worker::<TransactionStatuses>()?;
        self.spawn_worker::<OwnedTransactions>()?;

        let was_cancelled = self.task_manager.wait().await?.into_iter().all(|e| e);

        Ok(was_cancelled)
    }

    pub fn spawn_worker<T>(&mut self) -> anyhow::Result<()>
    where
        T: TableWithBlueprint + 'static + Send,
        TableEntry<T>: serde::de::DeserializeOwned + Send,
        StateConfig: AsTable<T>,
        Handler: ImportTable<T>,
    {
        let groups = self.snapshot_reader.read::<T>()?;
        let num_groups = groups.len();

        let block_height = self.snapshot_reader.block_height();
        let da_block_height = self.snapshot_reader.da_block_height();
        let on_chain_db = self.db.on_chain().clone();
        let off_chain_db = self.db.off_chain().clone();

        let progress_reporter = self.progress_reporter::<T>(num_groups);

        self.task_manager.spawn(move |token| {
            tokio_rayon::spawn(move || {
                import_task::import_entries(
                    token,
                    Handler::new(block_height, da_block_height),
                    groups,
                    on_chain_db,
                    off_chain_db,
                    progress_reporter,
                )
            })
        });

        Ok(())
    }

    fn init_multi_progress_reporter() -> MultipleProgressReporter {
        if Self::should_display_bars() {
            MultipleProgressReporter::new_sterr()
        } else {
            MultipleProgressReporter::new_hidden()
        }
    }

    fn should_display_bars() -> bool {
        std::io::stderr().is_terminal() && !cfg!(test)
    }

    fn progress_reporter<T>(&self, num_groups: usize) -> ProgressReporter
    where
        T: TableWithBlueprint,
    {
        let target = if Self::should_display_bars() {
            Target::Cli(T::column().name())
        } else {
            let span = tracing::span!(
                parent: &self.tracing_span,
                Level::INFO,
                "task",
                table = T::column().name()
            );
            Target::Logs(span)
        };

        let reporter = ProgressReporter::new(target, num_groups);
        self.multi_progress_reporter.register(reporter)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Handler {
    pub block_height: BlockHeight,
    pub da_block_height: DaBlockHeight,
}

impl Handler {
    pub fn new(block_height: BlockHeight, da_block_height: DaBlockHeight) -> Self {
        Self {
            block_height,
            da_block_height,
        }
    }
}
