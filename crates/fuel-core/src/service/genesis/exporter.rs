use crate::{
    combined_database::CombinedDatabase,
    database::{
        database_description::{
            off_chain::OffChain, on_chain::OnChain, DatabaseDescription,
        },
        Database, EntriesFilter,
    },
    fuel_core_graphql_api::storage::transactions::{
        OwnedTransactions, TransactionStatuses,
    },
};
use fuel_core_chain_config::{
    AddTable, ChainConfig, SnapshotFragment, SnapshotWriter, StateConfigBuilder,
    TableEntry,
};
use fuel_core_storage::{
    blueprint::BlueprintInspect,
    iter::IterDirection,
    structured_storage::TableWithBlueprint,
    tables::{
        Coins, ContractsAssets, ContractsLatestUtxo, ContractsRawCode, ContractsState,
        Messages, Transactions,
    },
    ContractsAssetKey, ContractsStateKey,
};
use fuel_core_types::fuel_types::ContractId;
use itertools::Itertools;

use tokio_util::sync::CancellationToken;

use super::task_manager::TaskManager;

pub struct Exporter {
    db: CombinedDatabase,
    prev_chain_config: ChainConfig,
    writer: Box<dyn Fn() -> anyhow::Result<SnapshotWriter>>,
    group_size: usize,
}

impl Exporter {
    pub fn new(
        db: CombinedDatabase,
        prev_chain_config: ChainConfig,
        writer: impl Fn() -> anyhow::Result<SnapshotWriter> + 'static,
        group_size: usize,
    ) -> Self {
        Self {
            db,
            prev_chain_config,
            writer: Box::new(writer),
            group_size,
        }
    }

    pub async fn write_full_snapshot(self) -> Result<(), anyhow::Error> {
        let mut task_manager: TaskManager<SnapshotFragment> =
            TaskManager::new(CancellationToken::new());

        let db = self.db.on_chain();
        self.spawn_task::<Coins, OnChain>(
            EntriesFilter::none(),
            &mut task_manager,
            db.clone(),
        )?;
        self.spawn_task::<Messages, OnChain>(
            EntriesFilter::none(),
            &mut task_manager,
            db.clone(),
        )?;
        self.spawn_task::<ContractsRawCode, OnChain>(
            EntriesFilter::none(),
            &mut task_manager,
            db.clone(),
        )?;
        self.spawn_task::<ContractsLatestUtxo, OnChain>(
            EntriesFilter::none(),
            &mut task_manager,
            db.clone(),
        )?;
        self.spawn_task::<ContractsState, OnChain>(
            EntriesFilter::none(),
            &mut task_manager,
            db.clone(),
        )?;
        self.spawn_task::<ContractsAssets, OnChain>(
            EntriesFilter::none(),
            &mut task_manager,
            db.clone(),
        )?;
        self.spawn_task::<Transactions, OnChain>(
            EntriesFilter::none(),
            &mut task_manager,
            db.clone(),
        )?;

        let db = self.db.off_chain();
        self.spawn_task::<TransactionStatuses, OffChain>(
            EntriesFilter::none(),
            &mut task_manager,
            db.clone(),
        )?;
        self.spawn_task::<OwnedTransactions, OffChain>(
            EntriesFilter::none(),
            &mut task_manager,
            db.clone(),
        )?;

        let data_snapshot_fragments = task_manager.wait().await?;

        let other_fragments =
            self.write_chain_config()?.merge(self.write_block_data()?)?;

        data_snapshot_fragments
            .into_iter()
            .try_fold(other_fragments, |fragment, next_fragment| {
                fragment.merge(next_fragment)
            })?
            .finalize()?;

        Ok(())
    }

    pub async fn write_contract_snapshot(
        self,
        contract_id: ContractId,
    ) -> Result<(), anyhow::Error> {
        let mut task_manager: TaskManager<SnapshotFragment> =
            TaskManager::new(CancellationToken::new());
        let db = self.db.on_chain();
        let prefix = contract_id.as_ref().to_vec();
        self.spawn_task::<ContractsRawCode, OnChain>(
            EntriesFilter::new(prefix.clone(), move |key| *key == contract_id),
            &mut task_manager,
            db.clone(),
        )?;

        self.spawn_task::<ContractsLatestUtxo, OnChain>(
            EntriesFilter::new(prefix.clone(), move |key| *key == contract_id),
            &mut task_manager,
            db.clone(),
        )?;

        self.spawn_task::<ContractsState, OnChain>(
            EntriesFilter::new(prefix.clone(), move |key: &ContractsStateKey| {
                *key.contract_id() == contract_id
            }),
            &mut task_manager,
            db.clone(),
        )?;

        self.spawn_task::<ContractsAssets, OnChain>(
            EntriesFilter::new(prefix.clone(), move |key: &ContractsAssetKey| {
                *key.contract_id() == contract_id
            }),
            &mut task_manager,
            db.clone(),
        )?;

        let block = self.db.on_chain().latest_block()?;

        let mut writer = self.create_writer()?;
        writer.write_block_data(*block.header().height(), block.header().da_height)?;
        writer.write_chain_config(&self.prev_chain_config);

        let chain_config_fragment = writer.partial_close()?;

        let fragments = task_manager.wait().await?;

        fragments
            .into_iter()
            .try_fold(chain_config_fragment, |fragment, next_fragment| {
                fragment.merge(next_fragment)
            })?
            .finalize()?;

        Ok(())
    }

    fn create_writer(&self) -> anyhow::Result<SnapshotWriter> {
        (self.writer)()
    }

    fn spawn_task<T, DbDesc>(
        &self,
        filter: EntriesFilter<T>,
        task_manager: &mut TaskManager<SnapshotFragment>,
        db: Database<DbDesc>,
    ) -> anyhow::Result<()>
    where
        T: TableWithBlueprint<Column = <DbDesc as DatabaseDescription>::Column> + 'static,
        T::Blueprint: BlueprintInspect<T, Database<DbDesc>>,
        TableEntry<T>: serde::Serialize,
        StateConfigBuilder: AddTable<T>,
        DbDesc: DatabaseDescription,
        DbDesc::Height: Send,
    {
        let mut writer = self.create_writer()?;
        let group_size = self.group_size;

        task_manager.spawn(move |cancel| {
            tokio_rayon::spawn(move || {
                db.entries::<T>(filter, IterDirection::Forward)
                    .chunks(group_size)
                    .into_iter()
                    .take_while(|_| !cancel.is_cancelled())
                    .try_for_each(|chunk| writer.write(chunk.try_collect()?))?;
                writer.partial_close()
            })
        });

        Ok(())
    }

    fn write_chain_config(&self) -> anyhow::Result<SnapshotFragment> {
        let mut writer = self.create_writer()?;
        writer.write_chain_config(&self.prev_chain_config);
        writer.partial_close()
    }

    fn write_block_data(&self) -> anyhow::Result<SnapshotFragment> {
        let mut writer = self.create_writer()?;
        let block = self.db.on_chain().latest_block()?;
        writer.write_block_data(*block.header().height(), block.header().da_height)?;
        writer.partial_close()
    }
}
