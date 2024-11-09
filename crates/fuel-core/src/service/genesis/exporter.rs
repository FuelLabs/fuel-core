use crate::{
    combined_database::CombinedDatabase,
    database::{
        database_description::DatabaseDescription,
        Database,
    },
    fuel_core_graphql_api::storage::{
        messages::SpentMessages,
        transactions::{
            OwnedTransactions,
            TransactionStatuses,
        },
    },
    graphql_api::storage::old::{
        OldFuelBlockConsensus,
        OldFuelBlocks,
        OldTransactions,
    },
};
use fuel_core_chain_config::{
    AddTable,
    ChainConfig,
    LastBlockConfig,
    SnapshotFragment,
    SnapshotMetadata,
    SnapshotWriter,
    StateConfigBuilder,
    TableEntry,
};
use fuel_core_poa::ports::Database as DatabaseTrait;
use fuel_core_storage::{
    iter::{
        IterDirection,
        IterableTable,
    },
    kv_store::StorageColumn,
    structured_storage::TableWithBlueprint,
    tables::{
        merkle::{
            FuelBlockMerkleData,
            FuelBlockMerkleMetadata,
        },
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
    transactional::AtomicView,
};
use fuel_core_types::{
    fuel_types::ContractId,
    fuel_vm::BlobData,
};
use itertools::Itertools;

use super::{
    progress::MultipleProgressReporter,
    task_manager::TaskManager,
    NotifyCancel,
};

pub struct Exporter<Fun> {
    db: CombinedDatabase,
    prev_chain_config: ChainConfig,
    writer: Fun,
    group_size: usize,
    task_manager: TaskManager<SnapshotFragment>,
    multi_progress: MultipleProgressReporter,
}

impl<Fun> Exporter<Fun>
where
    Fun: Fn() -> anyhow::Result<SnapshotWriter>,
{
    pub fn new(
        db: CombinedDatabase,
        prev_chain_config: ChainConfig,
        writer: Fun,
        group_size: usize,
        cancel_token: impl NotifyCancel + Send + Sync + 'static,
    ) -> Self {
        Self {
            db,
            prev_chain_config,
            writer,
            group_size,
            task_manager: TaskManager::new(cancel_token),
            multi_progress: MultipleProgressReporter::new(tracing::info_span!(
                "snapshot_exporter"
            )),
        }
    }

    pub async fn write_full_snapshot(mut self) -> Result<(), anyhow::Error> {
        macro_rules! export {
            ($db: expr, $($table: ty),*) => {
                $(self.spawn_task::<$table, _>(None, $db)?;)*
            };
        }

        export!(
            |ctx: &Self| ctx.db.on_chain(),
            Coins,
            Messages,
            BlobData,
            ContractsRawCode,
            ContractsLatestUtxo,
            ContractsState,
            ContractsAssets,
            FuelBlocks,
            FuelBlockMerkleData,
            FuelBlockMerkleMetadata,
            Transactions,
            SealedBlockConsensus,
            ProcessedTransactions
        );

        export!(
            |ctx: &Self| ctx.db.off_chain(),
            TransactionStatuses,
            OwnedTransactions,
            OldFuelBlocks,
            OldFuelBlockConsensus,
            OldTransactions,
            SpentMessages
        );

        self.finalize().await?;

        Ok(())
    }

    pub async fn write_contract_snapshot(
        mut self,
        contract_id: ContractId,
    ) -> Result<(), anyhow::Error> {
        macro_rules! export {
            ($($table: ty),*) => {
                $(self.spawn_task::<$table, _>(Some(contract_id.as_ref()), |ctx: &Self| ctx.db.on_chain())?;)*
            };
        }
        export!(
            ContractsAssets,
            ContractsState,
            ContractsLatestUtxo,
            ContractsRawCode
        );

        self.finalize().await?;

        Ok(())
    }

    async fn finalize(self) -> anyhow::Result<SnapshotMetadata> {
        let writer = self.create_writer()?;
        let view = self.db.on_chain().latest_view()?;
        let latest_block = view.latest_block()?;
        let blocks_root =
            view.block_header_merkle_root(latest_block.header().height())?;
        let latest_block =
            LastBlockConfig::from_header(latest_block.header(), blocks_root);

        let writer_fragment = writer.partial_close()?;
        self.task_manager
            .wait()
            .await?
            .into_iter()
            .try_fold(writer_fragment, |fragment, next_fragment| {
                fragment.merge(next_fragment)
            })?
            .finalize(Some(latest_block), &self.prev_chain_config)
    }

    fn create_writer(&self) -> anyhow::Result<SnapshotWriter> {
        (self.writer)()
    }

    fn spawn_task<T, DbDesc>(
        &mut self,
        prefix: Option<&[u8]>,
        db_picker: impl FnOnce(&Self) -> &Database<DbDesc>,
    ) -> anyhow::Result<()>
    where
        T: TableWithBlueprint + 'static + Send + Sync,
        TableEntry<T>: serde::Serialize,
        StateConfigBuilder: AddTable<T>,
        DbDesc: DatabaseDescription,
        Database<DbDesc>: IterableTable<T>,
    {
        let mut writer = self.create_writer()?;
        let group_size = self.group_size;

        let db = db_picker(self).clone();
        let prefix = prefix.map(|p| p.to_vec());
        // TODO:
        // [1857](https://github.com/FuelLabs/fuel-core/issues/1857)
        // RocksDb can provide an estimate for the number of items.
        let progress_tracker =
            self.multi_progress.table_reporter(None, T::column().name());
        self.task_manager.spawn_blocking(move |cancel| {
            db.entries::<T>(prefix, IterDirection::Forward)
                .chunks(group_size)
                .into_iter()
                .take_while(|_| !cancel.is_cancelled())
                .enumerate()
                .try_for_each(|(index, chunk)| {
                    progress_tracker.set_index(index);

                    writer.write(chunk.try_collect()?)
                })?;
            writer.partial_close()
        });

        Ok(())
    }
}
