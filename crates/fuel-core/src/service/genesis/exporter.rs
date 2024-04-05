use crate::{
    combined_database::CombinedDatabase,
    database::{
        database_description::DatabaseDescription,
        Database,
        IncludeAll,
        KeyFilter,
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
    SnapshotMetadata,
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

use self::filter::ByContractId;

use super::task_manager::TaskManager;
mod filter;

pub struct Exporter<Fun> {
    db: CombinedDatabase,
    prev_chain_config: ChainConfig,
    writer: Fun,
    group_size: usize,
    task_manager: TaskManager<SnapshotFragment>,
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
    ) -> Self {
        Self {
            db,
            prev_chain_config,
            writer,
            group_size,
            task_manager: TaskManager::new(CancellationToken::new()),
        }
    }

    pub async fn write_full_snapshot(mut self) -> Result<(), anyhow::Error> {
        macro_rules! export {
            ($db: expr, $($table: ty),*) => {
                $(self.spawn_task::<$table, _>(IncludeAll, $db)?;)*
            };
        }

        export!(
            |ctx: &Self| ctx.db.on_chain(),
            Coins,
            Messages,
            ContractsRawCode,
            ContractsLatestUtxo,
            ContractsState,
            ContractsAssets,
            Transactions
        );

        export!(
            |ctx: &Self| ctx.db.off_chain(),
            TransactionStatuses,
            OwnedTransactions
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
                let filter = ByContractId::new(contract_id);
                $(self.spawn_task::<$table, _>(filter, |ctx: &Self| ctx.db.on_chain())?;)*
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
        let remaining_fragment = self.write_block_and_chain_config()?;
        self.task_manager
            .wait()
            .await?
            .into_iter()
            .try_fold(remaining_fragment, |fragment, next_fragment| {
                fragment.merge(next_fragment)
            })?
            .finalize()
    }

    fn write_block_and_chain_config(&self) -> anyhow::Result<SnapshotFragment> {
        let mut writer = self.create_writer()?;
        writer.write_chain_config(&self.prev_chain_config);

        let block = self.db.on_chain().latest_block()?;
        writer.write_block_data(*block.header().height(), block.header().da_height)?;

        writer.partial_close()
    }

    fn create_writer(&self) -> anyhow::Result<SnapshotWriter> {
        (self.writer)()
    }

    fn spawn_task<T, DbDesc>(
        &mut self,
        filter: impl KeyFilter<T::OwnedKey> + Send + 'static,
        db_picker: impl FnOnce(&Self) -> &Database<DbDesc>,
    ) -> anyhow::Result<()>
    where
        T: TableWithBlueprint + 'static + Send + Sync,
        T::Blueprint: BlueprintInspect<T, Database<DbDesc>>,
        TableEntry<T>: serde::Serialize,
        StateConfigBuilder: AddTable<T>,
        DbDesc: DatabaseDescription<Column = T::Column>,
        DbDesc::Height: Send,
    {
        let mut writer = self.create_writer()?;
        let group_size = self.group_size;

        let db = db_picker(self).clone();
        self.task_manager.spawn(move |cancel| {
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
}
