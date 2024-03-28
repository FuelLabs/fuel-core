use crate::cli::default_db_path;
use anyhow::Context;
use clap::{
    Parser,
    Subcommand,
};
use fuel_core::{
    chain_config::ChainConfig,
    combined_database::CombinedDatabase,
    database::{
        database_description::{
            on_chain::OnChain,
            DatabaseDescription,
        },
        Database,
    },
    types::fuel_types::ContractId,
};
use fuel_core_chain_config::{
    AddTable,
    SnapshotWriter,
    StateConfigBuilder,
    TableEntry,
    MAX_GROUP_SIZE,
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
    },
};
use itertools::Itertools;
use std::path::{
    Path,
    PathBuf,
};

/// Print a snapshot of blockchain state to stdout.
#[derive(Debug, Clone, Parser)]
pub struct Command {
    /// The path to the database.
    #[clap(
        name = "DB_PATH",
        long = "db-path",
        value_parser,
        default_value = default_db_path().into_os_string()
    )]
    pub(crate) database_path: PathBuf,

    /// Where to save the snapshot
    #[arg(name = "OUTPUT_DIR", long = "output-directory")]
    pub(crate) output_dir: PathBuf,

    /// The maximum database cache size in bytes.
    #[arg(
        long = "max-database-cache-size",
        default_value_t = super::DEFAULT_DATABASE_CACHE_SIZE,
        env
    )]
    pub max_database_cache_size: usize,

    /// The sub-command of the snapshot operation.
    #[command(subcommand)]
    pub(crate) subcommand: SubCommands,
}

#[derive(Subcommand, Debug, Clone, Copy)]
pub enum Encoding {
    Json,
    #[cfg(feature = "parquet")]
    Parquet {
        /// The number of entries to write per parquet group.
        #[clap(name = "GROUP_SIZE", long = "group-size", default_value = "10000")]
        group_size: usize,
        /// Level of compression. Valid values are 0..=12.
        #[clap(
            name = "COMPRESSION_LEVEL",
            long = "compression-level",
            default_value = "1"
        )]
        compression: u8,
    },
}

impl Encoding {
    fn group_size(self) -> Option<usize> {
        match self {
            Encoding::Json => None,
            #[cfg(feature = "parquet")]
            Encoding::Parquet { group_size, .. } => Some(group_size),
        }
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum EncodingCommand {
    /// The encoding format for the chain state files.
    Encoding {
        #[clap(subcommand)]
        encoding: Encoding,
    },
}

impl EncodingCommand {
    fn encoding(self) -> Encoding {
        match self {
            EncodingCommand::Encoding { encoding } => encoding,
        }
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum SubCommands {
    /// Creates a snapshot of the entire database and produces a chain config.
    Everything {
        /// Specify a path to the chain config. Defaults used if no path
        /// is provided.
        #[clap(name = "CHAIN_CONFIG", long = "chain")]
        chain_config: Option<PathBuf>,
        /// Encoding format for the chain state files.
        #[clap(subcommand)]
        encoding_command: Option<EncodingCommand>,
    },
    /// Creates a config for the contract.
    #[command(arg_required_else_help = true)]
    Contract {
        /// The id of the contract to snapshot.
        #[clap(long = "id")]
        contract_id: ContractId,
    },
}

#[cfg(any(feature = "rocksdb", feature = "rocksdb-production"))]
pub fn exec(command: Command) -> anyhow::Result<()> {
    let db = open_db(
        &command.database_path,
        Some(command.max_database_cache_size),
    )?;
    let output_dir = command.output_dir;

    match command.subcommand {
        SubCommands::Everything {
            chain_config,
            encoding_command,
            ..
        } => {
            let encoding = encoding_command
                .map(|f| f.encoding())
                .unwrap_or_else(|| Encoding::Json);
            full_snapshot(chain_config, &output_dir, encoding, db)
        }
        SubCommands::Contract { contract_id } => {
            contract_snapshot(db, contract_id, &output_dir)
        }
    }
}

fn contract_snapshot(
    db: CombinedDatabase,
    contract_id: ContractId,
    output_dir: &Path,
) -> Result<(), anyhow::Error> {
    std::fs::create_dir_all(output_dir)?;

    let code = db
        .on_chain()
        .entries::<ContractsRawCode>(Some(contract_id.as_ref()), IterDirection::Forward)
        .next()
        .ok_or_else(|| {
            anyhow::anyhow!("contract code not found! id: {:?}", contract_id)
        })??;

    let utxo = db
        .on_chain()
        .entries::<ContractsLatestUtxo>(
            Some(contract_id.as_ref()),
            IterDirection::Forward,
        )
        .next()
        .ok_or_else(|| {
            anyhow::anyhow!("contract utxo not found! id: {:?}", contract_id)
        })??;

    let state = db
        .on_chain()
        .entries::<ContractsState>(Some(contract_id.as_ref()), IterDirection::Forward)
        .try_collect()?;

    let balance = db
        .on_chain()
        .entries::<ContractsAssets>(Some(contract_id.as_ref()), IterDirection::Forward)
        .try_collect()?;

    let block = db.on_chain().latest_block()?;
    let mut writer = SnapshotWriter::json(output_dir);

    writer.write(vec![code])?;
    writer.write(vec![utxo])?;
    writer.write(state)?;
    writer.write(balance)?;
    writer.write_block_data(*block.header().height(), block.header().da_height)?;
    writer.write_chain_config(&crate::cli::local_testnet_chain_config())?;
    writer.close()?;
    Ok(())
}

fn full_snapshot(
    prev_chain_config: Option<PathBuf>,
    output_dir: &Path,
    encoding: Encoding,
    db: CombinedDatabase,
) -> Result<(), anyhow::Error> {
    std::fs::create_dir_all(output_dir)?;

    let mut writer = match encoding {
        Encoding::Json => SnapshotWriter::json(output_dir),
        #[cfg(feature = "parquet")]
        Encoding::Parquet { compression, .. } => {
            SnapshotWriter::parquet(output_dir, compression.try_into()?)?
        }
    };

    let prev_chain_config = load_chain_config(prev_chain_config)?;
    writer.write_chain_config(&prev_chain_config)?;

    fn write<T>(
        db: &CombinedDatabase,
        group_size: usize,
        writer: &mut SnapshotWriter,
    ) -> anyhow::Result<()>
    where
        T: TableWithBlueprint<Column = <OnChain as DatabaseDescription>::Column>,
        T::Blueprint: BlueprintInspect<T, Database>,
        TableEntry<T>: serde::Serialize,
        StateConfigBuilder: AddTable<T>,
    {
        db.on_chain()
            .entries::<T>(None, IterDirection::Forward)
            .chunks(group_size)
            .into_iter()
            .try_for_each(|chunk| writer.write(chunk.try_collect()?))
    }
    let group_size = encoding.group_size().unwrap_or(MAX_GROUP_SIZE);

    write::<Coins>(&db, group_size, &mut writer)?;
    write::<Messages>(&db, group_size, &mut writer)?;
    write::<ContractsRawCode>(&db, group_size, &mut writer)?;
    write::<ContractsLatestUtxo>(&db, group_size, &mut writer)?;
    write::<ContractsState>(&db, group_size, &mut writer)?;
    write::<ContractsAssets>(&db, group_size, &mut writer)?;

    let block = db.on_chain().latest_block()?;
    writer.write_block_data(*block.header().height(), block.header().da_height)?;

    writer.close()?;

    Ok(())
}

fn load_chain_config(
    chain_config: Option<PathBuf>,
) -> Result<ChainConfig, anyhow::Error> {
    let chain_config = match chain_config {
        Some(file) => ChainConfig::load(file)?,
        None => crate::cli::local_testnet_chain_config(),
    };

    Ok(chain_config)
}

fn open_db(path: &Path, capacity: Option<usize>) -> anyhow::Result<CombinedDatabase> {
    CombinedDatabase::open(path, capacity.unwrap_or(1024 * 1024 * 1024))
        .map_err(Into::<anyhow::Error>::into)
        .context(format!("failed to open combined database at path {path:?}",))
}

#[cfg(test)]
mod tests {

    use std::iter::repeat_with;

    use fuel_core::database::Database;
    use fuel_core_chain_config::{
        AddTable,
        AsTable,
        SnapshotMetadata,
        SnapshotReader,
        StateConfig,
        StateConfigBuilder,
        TableEntry,
    };
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
        },
        transactional::{
            IntoTransaction,
            StorageTransaction,
        },
        ContractsAssetKey,
        ContractsStateKey,
        StorageAsMut,
    };
    use fuel_core_types::{
        blockchain::{
            block::CompressedBlock,
            primitives::DaBlockHeight,
        },
        entities::{
            coins::coin::{
                CompressedCoin,
                CompressedCoinV1,
            },
            contract::ContractUtxoInfo,
            relayer::message::{
                Message,
                MessageV1,
            },
        },
        fuel_tx::{
            TxPointer,
            UtxoId,
        },
    };
    use rand::{
        rngs::StdRng,
        seq::SliceRandom,
        Rng,
        SeedableRng,
    };
    use test_case::test_case;

    use crate::cli::DEFAULT_DATABASE_CACHE_SIZE;

    use super::*;

    struct DbPopulator {
        db: StorageTransaction<Database>,
        rng: StdRng,
    }

    #[derive(Debug, PartialEq)]
    struct OnChainData {
        coins: Vec<TableEntry<Coins>>,
        messages: Vec<TableEntry<Messages>>,
        contract_code: Vec<TableEntry<ContractsRawCode>>,
        contract_utxo: Vec<TableEntry<ContractsLatestUtxo>>,
        contract_state: Vec<TableEntry<ContractsState>>,
        contract_balance: Vec<TableEntry<ContractsAssets>>,
        block: TableEntry<FuelBlocks>,
    }

    impl OnChainData {
        fn sorted(mut self) -> OnChainData {
            self.coins.sort_by_key(|e| e.key);
            self.messages.sort_by_key(|e| e.key);
            self.contract_code.sort_by_key(|e| e.key);
            self.contract_utxo.sort_by_key(|e| e.key);
            self.contract_state.sort_by_key(|e| e.key);
            self.contract_balance.sort_by_key(|e| e.key);
            self
        }

        fn into_state_config(self) -> StateConfig {
            let mut builder = StateConfigBuilder::default();
            builder.add(self.coins);
            builder.add(self.messages);
            builder.add(self.contract_code);
            builder.add(self.contract_utxo);
            builder.add(self.contract_state);
            builder.add(self.contract_balance);

            let height = self.block.value.header().height();
            builder.set_block_height(*height);

            let da_height = self.block.value.header().application().da_height;
            builder.set_da_block_height(da_height);

            builder.build().unwrap()
        }

        fn read_from_snapshot(snapshot: SnapshotMetadata) -> Self {
            let mut reader = SnapshotReader::open(snapshot).unwrap();

            fn read<T>(reader: &mut SnapshotReader) -> Vec<TableEntry<T>>
            where
                T: TableWithBlueprint,
                StateConfig: AsTable<T>,
                TableEntry<T>: serde::de::DeserializeOwned,
            {
                reader
                    .read::<T>()
                    .unwrap()
                    .map_ok(|group| group.data)
                    .flatten_ok()
                    .try_collect()
                    .unwrap()
            }

            let block = {
                let mut block = CompressedBlock::default();
                let height = reader.block_height();
                block.header_mut().application_mut().da_height = reader.da_block_height();
                block.header_mut().set_block_height(height);
                TableEntry {
                    key: height,
                    value: block,
                }
            };

            let reader = &mut reader;

            Self {
                coins: read(reader),
                messages: read(reader),
                contract_code: read(reader),
                contract_utxo: read(reader),
                contract_state: read(reader),
                contract_balance: read(reader),
                block,
            }
        }
    }

    impl DbPopulator {
        fn new(db: CombinedDatabase, rng: StdRng) -> Self {
            Self {
                db: (db.on_chain().clone()).into_transaction(),
                rng,
            }
        }

        fn commit(self) {
            self.db.commit().expect("failed to commit transaction");
        }

        fn given_persisted_on_chain_data(
            &mut self,
            coins: usize,
            messages: usize,
            contracts: usize,
            states_per_contract: usize,
            balances_per_contract: usize,
        ) -> OnChainData {
            let coins = repeat_with(|| self.given_coin()).take(coins).collect();
            let messages = repeat_with(|| self.given_message())
                .take(messages)
                .collect();

            let contract_ids = repeat_with(|| {
                let contract_id: ContractId = self.rng.gen();
                contract_id
            })
            .take(contracts)
            .collect_vec();

            let contract_code = contract_ids
                .iter()
                .map(|id| self.given_contract_code(*id))
                .collect();

            let contract_utxo = contract_ids
                .iter()
                .map(|id| self.given_contract_utxo(*id))
                .collect();

            let contract_state = contract_ids
                .iter()
                .flat_map(|id| {
                    repeat_with(|| self.given_contract_state(*id))
                        .take(states_per_contract)
                        .collect_vec()
                })
                .collect();

            let contract_balance = contract_ids
                .iter()
                .flat_map(|id| {
                    repeat_with(|| self.given_contract_asset(*id))
                        .take(balances_per_contract)
                        .collect_vec()
                })
                .collect();

            let block = self.given_block();
            OnChainData {
                coins,
                messages,
                contract_code,
                contract_utxo,
                contract_state,
                contract_balance,
                block,
            }
        }

        fn given_block(&mut self) -> TableEntry<FuelBlocks> {
            let mut block = CompressedBlock::default();
            let height = self.rng.gen();
            block.header_mut().application_mut().da_height = self.rng.gen();
            block.header_mut().set_block_height(height);
            let _ = self
                .db
                .storage_as_mut::<FuelBlocks>()
                .insert(&height, &block);

            TableEntry {
                key: height,
                value: block,
            }
        }

        fn given_coin(&mut self) -> TableEntry<Coins> {
            let tx_id = self.rng.gen();
            let output_index = self.rng.gen();
            let coin = CompressedCoin::V1(CompressedCoinV1 {
                owner: self.rng.gen(),
                amount: self.rng.gen(),
                asset_id: self.rng.gen(),
                tx_pointer: self.rng.gen(),
            });
            let key = UtxoId::new(tx_id, output_index);
            self.db
                .storage_as_mut::<Coins>()
                .insert(&key, &coin)
                .unwrap();

            TableEntry { key, value: coin }
        }

        fn given_message(&mut self) -> TableEntry<Messages> {
            let message = Message::V1(MessageV1 {
                sender: self.rng.gen(),
                recipient: self.rng.gen(),
                amount: self.rng.gen(),
                nonce: self.rng.gen(),
                data: self.generate_data(100),
                da_height: DaBlockHeight(self.rng.gen()),
            });

            let key = *message.nonce();
            self.db
                .storage_as_mut::<Messages>()
                .insert(&key, &message)
                .unwrap();

            TableEntry {
                key,
                value: message,
            }
        }

        fn given_contract_code(
            &mut self,
            contract_id: ContractId,
        ) -> TableEntry<ContractsRawCode> {
            let key = contract_id;

            let code = self.generate_data(1000);
            self.db
                .storage_as_mut::<ContractsRawCode>()
                .insert(&key, code.as_ref())
                .unwrap();

            TableEntry {
                key,
                value: code.into(),
            }
        }

        fn given_contract_utxo(
            &mut self,
            contract_id: ContractId,
        ) -> TableEntry<ContractsLatestUtxo> {
            let utxo_id = UtxoId::new(self.rng.gen(), self.rng.gen());
            let tx_pointer = TxPointer::new(self.rng.gen(), self.rng.gen());

            let value = ContractUtxoInfo::V1((utxo_id, tx_pointer).into());
            self.db
                .storage::<ContractsLatestUtxo>()
                .insert(&contract_id, &value)
                .unwrap();

            TableEntry {
                key: contract_id,
                value,
            }
        }

        fn given_contract_state(
            &mut self,
            contract_id: ContractId,
        ) -> TableEntry<ContractsState> {
            let state_key = self.rng.gen();
            let key = ContractsStateKey::new(&contract_id, &state_key);
            let state_value = self.generate_data(100);
            self.db
                .storage_as_mut::<ContractsState>()
                .insert(&key, &state_value)
                .unwrap();
            TableEntry {
                key,
                value: state_value.into(),
            }
        }

        fn given_contract_asset(
            &mut self,
            contract_id: ContractId,
        ) -> TableEntry<ContractsAssets> {
            let asset_id = self.rng.gen();
            let key = ContractsAssetKey::new(&contract_id, &asset_id);
            let amount = self.rng.gen();
            self.db
                .storage_as_mut::<ContractsAssets>()
                .insert(&key, &amount)
                .unwrap();
            TableEntry { key, value: amount }
        }

        fn generate_data(&mut self, max_amount: usize) -> Vec<u8> {
            let mut data = vec![0u8; self.rng.gen_range(0..=max_amount)];
            self.rng.fill(data.as_mut_slice());
            data
        }
    }

    #[cfg_attr(feature = "parquet", test_case(Encoding::Parquet { group_size: 2, compression: 1 }; "parquet"))]
    #[test_case(Encoding::Json; "json")]
    fn everything_snapshot_correct_and_sorted(encoding: Encoding) -> anyhow::Result<()> {
        use pretty_assertions::assert_eq;

        // given
        let temp_dir = tempfile::tempdir()?;

        let snapshot_dir = temp_dir.path().join("snapshot");
        let db_path = temp_dir.path().join("db");
        let mut db = DbPopulator::new(open_db(&db_path, None)?, StdRng::seed_from_u64(2));

        let state = db.given_persisted_on_chain_data(10, 10, 10, 10, 10);
        db.commit();

        // when
        exec(Command {
            database_path: db_path,
            max_database_cache_size: DEFAULT_DATABASE_CACHE_SIZE,
            output_dir: snapshot_dir.clone(),
            subcommand: SubCommands::Everything {
                chain_config: None,
                encoding_command: Some(EncodingCommand::Encoding { encoding }),
            },
        })?;

        // then
        let snapshot = SnapshotMetadata::read(&snapshot_dir)?;

        let written_data = OnChainData::read_from_snapshot(snapshot);

        assert_eq!(written_data.block, state.block);

        assert_ne!(written_data, state);
        assert_eq!(written_data, state.sorted());

        Ok(())
    }

    #[cfg(feature = "parquet")]
    #[test_case(2; "parquet group_size=2")]
    #[test_case(5; "parquet group_size=5")]
    fn everything_snapshot_respects_group_size(group_size: usize) -> anyhow::Result<()> {
        use fuel_core_chain_config::SnapshotReader;

        // given
        let temp_dir = tempfile::tempdir()?;

        let snapshot_dir = temp_dir.path().join("snapshot");
        let db_path = temp_dir.path().join("db");
        let mut db = DbPopulator::new(open_db(&db_path, None)?, StdRng::seed_from_u64(2));

        let state = db.given_persisted_on_chain_data(10, 10, 10, 10, 10);
        db.commit();

        // when
        exec(Command {
            database_path: db_path,
            output_dir: snapshot_dir.clone(),
            max_database_cache_size: DEFAULT_DATABASE_CACHE_SIZE,
            subcommand: SubCommands::Everything {
                chain_config: None,
                encoding_command: Some(EncodingCommand::Encoding {
                    encoding: Encoding::Parquet {
                        group_size,
                        compression: 1,
                    },
                }),
            },
        })?;

        // then
        let snapshot = SnapshotMetadata::read(&snapshot_dir)?;
        let mut reader = SnapshotReader::open(snapshot)?;

        let expected_state = state.sorted();
        assert_groups_as_expected(group_size, expected_state.coins, &mut reader);

        Ok(())
    }

    #[test]
    fn contract_snapshot_isolates_contract_correctly() -> anyhow::Result<()> {
        // given
        let temp_dir = tempfile::tempdir()?;
        let snapshot_dir = temp_dir.path().join("snapshot");

        let db_path = temp_dir.path().join("db");
        let mut db = DbPopulator::new(open_db(&db_path, None)?, StdRng::seed_from_u64(2));

        let original_state = db
            .given_persisted_on_chain_data(10, 10, 10, 10, 10)
            .sorted()
            .into_state_config();

        let randomly_chosen_contract = original_state
            .contracts
            .choose(&mut db.rng)
            .unwrap()
            .clone();
        let contract_id = randomly_chosen_contract.contract_id;
        db.commit();

        // when
        exec(Command {
            database_path: db_path,
            output_dir: snapshot_dir.clone(),
            max_database_cache_size: DEFAULT_DATABASE_CACHE_SIZE,
            subcommand: SubCommands::Contract { contract_id },
        })?;

        // then
        let metadata = SnapshotMetadata::read(&snapshot_dir)?;
        let snapshot_state = StateConfig::from_snapshot_metadata(metadata)?;

        assert_eq!(
            snapshot_state,
            StateConfig {
                coins: vec![],
                messages: vec![],
                contracts: vec![randomly_chosen_contract],
                block_height: original_state.block_height,
                da_block_height: original_state.da_block_height,
            }
        );

        Ok(())
    }

    #[cfg(feature = "parquet")]
    fn assert_groups_as_expected<T>(
        expected_group_size: usize,
        expected_data: Vec<TableEntry<T>>,
        reader: &mut SnapshotReader,
    ) where
        T: TableWithBlueprint,
        T::OwnedKey: serde::de::DeserializeOwned + core::fmt::Debug + PartialEq,
        T::OwnedValue: serde::de::DeserializeOwned + core::fmt::Debug + PartialEq,
        StateConfig: AsTable<T>,
    {
        let actual = reader
            .read()
            .unwrap()
            .map(|group| group.unwrap().data)
            .collect_vec();

        let expected = expected_data
            .into_iter()
            .chunks(expected_group_size)
            .into_iter()
            .map(|chunk| chunk.collect_vec())
            .collect_vec();

        assert_eq!(actual, expected);
    }
}
