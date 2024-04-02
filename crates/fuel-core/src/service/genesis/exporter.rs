use crate::{
    combined_database::CombinedDatabase,
    database::{
        database_description::{
            off_chain::OffChain,
            on_chain::OnChain,
            DatabaseDescription,
        },
        Database,
    },
    fuel_core_graphql_api::storage::transactions::{
        OwnedTransactions,
        TransactionStatuses,
    },
};
use fuel_core_chain_config::{
    AddTable,
    ChainConfig,
    SnapshotFragment,
    SnapshotWriter,
    StateConfigBuilder,
    TableEntry,
};
use fuel_core_storage::{
    blueprint::BlueprintInspect,
    iter::IterDirection,
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
        self.write::<Coins, OnChain>(&mut task_manager, db.clone())?;
        self.write::<Messages, OnChain>(&mut task_manager, db.clone())?;
        self.write::<ContractsRawCode, OnChain>(&mut task_manager, db.clone())?;
        self.write::<ContractsLatestUtxo, OnChain>(&mut task_manager, db.clone())?;
        self.write::<ContractsState, OnChain>(&mut task_manager, db.clone())?;
        self.write::<ContractsAssets, OnChain>(&mut task_manager, db.clone())?;
        self.write::<Transactions, OnChain>(&mut task_manager, db.clone())?;

        let db = self.db.off_chain();
        self.write::<TransactionStatuses, OffChain>(&mut task_manager, db.clone())?;
        self.write::<OwnedTransactions, OffChain>(&mut task_manager, db.clone())?;

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

    pub fn write_contract_snapshot(
        self,
        contract_id: ContractId,
    ) -> Result<(), anyhow::Error> {
        let code = self
            .db
            .on_chain()
            .entries::<ContractsRawCode>(
                Some(contract_id.as_ref()),
                IterDirection::Forward,
            )
            .next()
            .ok_or_else(|| {
                anyhow::anyhow!("contract code not found! id: {:?}", contract_id)
            })??;

        let utxo = self
            .db
            .on_chain()
            .entries::<ContractsLatestUtxo>(
                Some(contract_id.as_ref()),
                IterDirection::Forward,
            )
            .next()
            .ok_or_else(|| {
                anyhow::anyhow!("contract utxo not found! id: {:?}", contract_id)
            })??;

        let state = self
            .db
            .on_chain()
            .entries::<ContractsState>(Some(contract_id.as_ref()), IterDirection::Forward)
            .try_collect()?;

        let balance = self
            .db
            .on_chain()
            .entries::<ContractsAssets>(
                Some(contract_id.as_ref()),
                IterDirection::Forward,
            )
            .try_collect()?;

        let block = self.db.on_chain().latest_block()?;
        let mut writer = self.create_writer()?;

        writer.write(vec![code])?;
        writer.write(vec![utxo])?;
        writer.write(state)?;
        writer.write(balance)?;
        writer.write_block_data(*block.header().height(), block.header().da_height)?;
        writer.write_chain_config(&self.prev_chain_config);
        writer.close()?;
        Ok(())
    }

    fn create_writer(&self) -> anyhow::Result<SnapshotWriter> {
        (self.writer)()
    }

    fn write<T, DbDesc>(
        &self,
        task_manager: &mut TaskManager<SnapshotFragment>,
        db: Database<DbDesc>,
    ) -> anyhow::Result<()>
    where
        T: TableWithBlueprint<Column = <DbDesc as DatabaseDescription>::Column>,
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
                db.entries::<T>(None, IterDirection::Forward)
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
