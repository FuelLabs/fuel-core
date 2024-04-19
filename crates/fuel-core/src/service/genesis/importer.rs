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
    fuel_core_graphql_api::storage::messages::SpentMessages,
    graphql_api::storage::{
        coins::OwnedCoins,
        contracts::ContractsInfo,
        messages::OwnedMessageIds,
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
        self.spawn_worker_on_chain::<Coins>()?;
        self.spawn_worker_on_chain::<Messages>()?;
        self.spawn_worker_on_chain::<ContractsRawCode>()?;
        self.spawn_worker_on_chain::<ContractsLatestUtxo>()?;
        self.spawn_worker_on_chain::<ContractsState>()?;
        self.spawn_worker_on_chain::<ContractsAssets>()?;
        self.spawn_worker_on_chain::<ProcessedTransactions>()?;

        self.spawn_worker_off_chain::<TransactionStatuses, TransactionStatuses>()?;
        self.spawn_worker_off_chain::<OwnedTransactions, OwnedTransactions>()?;
        self.spawn_worker_off_chain::<SpentMessages, SpentMessages>()?;
        self.spawn_worker_off_chain::<Messages, OwnedMessageIds>()?;
        self.spawn_worker_off_chain::<Coins, OwnedCoins>()?;
        self.spawn_worker_off_chain::<FuelBlocks, OldFuelBlocks>()?;
        self.spawn_worker_off_chain::<Transactions, OldTransactions>()?;
        self.spawn_worker_off_chain::<SealedBlockConsensus, OldFuelBlockConsensus>()?;
        self.spawn_worker_off_chain::<Transactions, ContractsInfo>()?;
        self.spawn_worker_off_chain::<OldTransactions, ContractsInfo>()?;
        self.spawn_worker_off_chain::<OldFuelBlocks, OldFuelBlocks>()?;
        self.spawn_worker_off_chain::<OldFuelBlockConsensus, OldFuelBlockConsensus>()?;
        self.spawn_worker_off_chain::<OldTransactions, OldTransactions>()?;

        self.task_manager.wait().await?;

        Ok(())
    }

    pub fn spawn_worker_on_chain<TableBeingWritten>(&mut self) -> anyhow::Result<()>
    where
        TableBeingWritten: TableWithBlueprint + 'static + Send,
        TableEntry<TableBeingWritten>: serde::de::DeserializeOwned + Send,
        StateConfig: AsTable<TableBeingWritten>,
        Handler<TableBeingWritten, TableBeingWritten>:
            ImportTable<TableInSnapshot = TableBeingWritten, DbDesc = OnChain>,
    {
        let groups = self.snapshot_reader.read::<TableBeingWritten>()?;
        let num_groups = groups.len();

        if num_groups == 0 {
            return Ok(());
        }

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
            async move {
                tokio_rayon::spawn(move || task.run()).await?;
                Ok(())
            }
        });

        Ok(())
    }

    pub fn spawn_worker_off_chain<TableInSnapshot, TableBeingWritten>(
        &mut self,
    ) -> anyhow::Result<()>
    where
        TableInSnapshot: TableWithBlueprint + Send + 'static,
        TableEntry<TableInSnapshot>: serde::de::DeserializeOwned + Send,
        StateConfig: AsTable<TableInSnapshot>,
        Handler<TableBeingWritten, TableInSnapshot>:
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
pub struct Handler<TableBeingWritten, TableInSnapshot> {
    pub block_height: BlockHeight,
    pub da_block_height: DaBlockHeight,
    _table_being_written: PhantomData<TableBeingWritten>,
    _table_in_snapshot: PhantomData<TableInSnapshot>,
}

impl<A, B> Handler<A, B> {
    pub fn new(block_height: BlockHeight, da_block_height: DaBlockHeight) -> Self {
        Self {
            block_height,
            da_block_height,
            _table_being_written: PhantomData,
            _table_in_snapshot: PhantomData,
        }
    }
}
