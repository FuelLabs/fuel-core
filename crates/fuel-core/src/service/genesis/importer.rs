use self::import_task::ImportTable;

use super::{
    progress::MultipleProgressReporter,
    task_manager::TaskManager,
};
mod import_task;
mod logic;

use crate::{
    combined_database::CombinedDatabase,
    fuel_core_graphql_api::storage::messages::SpentMessages,
    graphql_api::storage::{
        old::{
            OldFuelBlockConsensus,
            OldFuelBlocks,
            OldTransactions,
        },
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
        FuelBlocks,
        Messages,
        ProcessedTransactions,
        SealedBlockConsensus,
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
        macro_rules! spawn_workers {
            ($($table:ty),*) => {
                $(self.spawn_worker::<$table>()?;)*
            };
        }
        spawn_workers!(
            Coins,
            ContractsAssets,
            ContractsLatestUtxo,
            ContractsRawCode,
            ContractsState,
            FuelBlocks,
            Messages,
            OldFuelBlockConsensus,
            OldFuelBlocks,
            OldTransactions,
            OwnedTransactions,
            ProcessedTransactions,
            SealedBlockConsensus,
            SpentMessages,
            TransactionStatuses,
            Transactions
        );

        self.task_manager.wait().await?;

        Ok(())
    }

    pub fn spawn_worker<TableInSnapshot>(&mut self) -> anyhow::Result<()>
    where
        TableInSnapshot: TableWithBlueprint + 'static + Send,
        TableEntry<TableInSnapshot>: serde::de::DeserializeOwned + Send,
        StateConfig: AsTable<TableInSnapshot>,
        Handler: ImportTable<TableInSnapshot>,
    {
        let groups = self.snapshot_reader.read::<TableInSnapshot>()?;
        let num_groups = groups.len();

        let block_height = *self.genesis_block.header().height();
        let da_block_height = self.genesis_block.header().da_height;

        let on_chain_db = self.db.on_chain().clone();
        let off_chain_db = self.db.off_chain().clone();

        let progress_reporter = self
            .multi_progress_reporter
            .table_reporter::<TableInSnapshot>(Some(num_groups));

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
