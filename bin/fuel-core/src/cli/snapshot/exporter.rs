use super::Encoding;
use fuel_core::combined_database::CombinedDatabase;
use fuel_core::database::database_description::off_chain::OffChain;
use fuel_core::database::database_description::on_chain::OnChain;
use fuel_core::database::database_description::DatabaseDescription;
use fuel_core::database::Database;
use fuel_core::fuel_core_graphql_api::storage::transactions::OwnedTransactions;
use fuel_core::fuel_core_graphql_api::storage::transactions::TransactionStatuses;
use fuel_core_chain_config::AddTable;
use fuel_core_chain_config::ChainConfig;
use fuel_core_chain_config::SnapshotFragment;
use fuel_core_chain_config::SnapshotWriter;
use fuel_core_chain_config::StateConfigBuilder;
use fuel_core_chain_config::TableEntry;
use fuel_core_chain_config::TaskManager;
use fuel_core_chain_config::MAX_GROUP_SIZE;
use fuel_core_storage::blueprint::BlueprintInspect;
use fuel_core_storage::iter::IterDirection;
use fuel_core_storage::structured_storage::TableWithBlueprint;
use fuel_core_storage::tables::Coins;
use fuel_core_storage::tables::ContractsAssets;
use fuel_core_storage::tables::ContractsLatestUtxo;
use fuel_core_storage::tables::ContractsRawCode;
use fuel_core_storage::tables::ContractsState;
use fuel_core_storage::tables::Messages;
use fuel_core_storage::tables::Transactions;
use fuel_core_types::fuel_types::ContractId;
use itertools::Itertools;

use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

pub struct Exporter {
    db: CombinedDatabase,
    output_dir: PathBuf,
    prev_chain_config: Option<PathBuf>,
    encoding: Encoding,
}

impl Exporter {
    pub fn new(
        db: CombinedDatabase,
        output_dir: PathBuf,
        prev_chain_config: Option<PathBuf>,
        encoding: Encoding,
    ) -> Self {
        Self {
            db,
            output_dir,
            prev_chain_config,
            encoding,
        }
    }

    pub async fn write_full_snapshot(self) -> Result<(), anyhow::Error> {
        std::fs::create_dir_all(&self.output_dir)?;

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
        std::fs::create_dir_all(&self.output_dir)?;

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
        let mut writer = SnapshotWriter::json(&self.output_dir);

        writer.write(vec![code])?;
        writer.write(vec![utxo])?;
        writer.write(state)?;
        writer.write(balance)?;
        writer.write_block_data(*block.header().height(), block.header().da_height)?;
        writer.write_chain_config(&crate::cli::local_testnet_chain_config())?;
        writer.close()?;
        Ok(())
    }

    fn create_writer(&self) -> anyhow::Result<SnapshotWriter> {
        let writer = match self.encoding {
            Encoding::Json => SnapshotWriter::json(&self.output_dir),
            #[cfg(feature = "parquet")]
            Encoding::Parquet { compression, .. } => {
                SnapshotWriter::parquet(&self.output_dir, compression.try_into()?)?
            }
        };
        Ok(writer)
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
        let group_size = self.encoding.group_size().unwrap_or(MAX_GROUP_SIZE);

        task_manager.spawn(move |cancel| async move {
            db.entries::<T>(None, IterDirection::Forward)
                .chunks(group_size)
                .into_iter()
                .take_while(|_| !cancel.is_cancelled())
                .try_for_each(|chunk| writer.write(chunk.try_collect()?))?;
            writer.partial_close()
        });

        Ok(())
    }

    fn load_chain_config(&self) -> anyhow::Result<ChainConfig> {
        let chain_config = match &self.prev_chain_config {
            Some(file) => ChainConfig::load(file)?,
            None => crate::cli::local_testnet_chain_config(),
        };

        Ok(chain_config)
    }

    fn write_chain_config(&self) -> anyhow::Result<SnapshotFragment> {
        let mut writer = self.create_writer()?;
        let prev_chain_config = self.load_chain_config()?;
        writer.write_chain_config(&prev_chain_config)?;
        writer.partial_close()
    }

    fn write_block_data(&self) -> anyhow::Result<SnapshotFragment> {
        let mut writer = self.create_writer()?;
        let block = self.db.on_chain().latest_block()?;
        writer.write_block_data(*block.header().height(), block.header().da_height)?;
        writer.partial_close()
    }
}
