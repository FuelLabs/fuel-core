use crate::cli::default_db_path;
use anyhow::Context;
use clap::{
    Parser,
    Subcommand,
};
use fuel_core::{
    combined_database::CombinedDatabase,
    state::historical_rocksdb::StateRewindPolicy,
    types::fuel_types::ContractId,
};
use fuel_core_chain_config::ChainConfig;
use std::path::{
    Path,
    PathBuf,
};

use super::local_testnet_chain_config;

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
    pub database_path: PathBuf,

    /// Where to save the snapshot
    #[arg(name = "OUTPUT_DIR", long = "output-directory")]
    pub output_dir: PathBuf,

    /// The maximum database cache size in bytes.
    #[arg(
        long = "max-database-cache-size",
        default_value_t = super::DEFAULT_DATABASE_CACHE_SIZE,
        env
    )]
    pub max_database_cache_size: usize,

    /// The sub-command of the snapshot operation.
    #[command(subcommand)]
    pub subcommand: SubCommands,
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

#[cfg(feature = "rocksdb")]
pub async fn exec(command: Command) -> anyhow::Result<()> {
    use fuel_core::service::genesis::Exporter;
    use fuel_core_chain_config::{
        SnapshotWriter,
        MAX_GROUP_SIZE,
    };

    use crate::cli::ShutdownListener;

    let db = open_db(
        &command.database_path,
        Some(command.max_database_cache_size),
    )?;
    let output_dir = command.output_dir;
    let shutdown_listener = ShutdownListener::spawn();

    match command.subcommand {
        SubCommands::Everything {
            chain_config,
            encoding_command,
            ..
        } => {
            let encoding = encoding_command
                .map(|f| f.encoding())
                .unwrap_or_else(|| Encoding::Json);

            let group_size = encoding.group_size().unwrap_or(MAX_GROUP_SIZE);
            let writer = move || match encoding {
                Encoding::Json => Ok(SnapshotWriter::json(output_dir.clone())),
                #[cfg(feature = "parquet")]
                Encoding::Parquet { compression, .. } => {
                    SnapshotWriter::parquet(output_dir.clone(), compression.try_into()?)
                }
            };
            Exporter::new(
                db,
                load_chain_config_or_use_testnet(chain_config.as_deref())?,
                writer,
                group_size,
                shutdown_listener,
            )
            .write_full_snapshot()
            .await
        }
        SubCommands::Contract { contract_id } => {
            let writer = move || Ok(SnapshotWriter::json(output_dir.clone()));
            Exporter::new(
                db,
                local_testnet_chain_config(),
                writer,
                MAX_GROUP_SIZE,
                shutdown_listener,
            )
            .write_contract_snapshot(contract_id)
            .await
        }
    }
}

fn load_chain_config_or_use_testnet(path: Option<&Path>) -> anyhow::Result<ChainConfig> {
    if let Some(path) = path {
        ChainConfig::load(path)
    } else {
        Ok(local_testnet_chain_config())
    }
}

fn open_db(path: &Path, capacity: Option<usize>) -> anyhow::Result<CombinedDatabase> {
    CombinedDatabase::open(
        path,
        capacity.unwrap_or(1024 * 1024 * 1024),
        StateRewindPolicy::NoRewind,
    )
    .map_err(Into::<anyhow::Error>::into)
    .context(format!("failed to open combined database at path {path:?}",))
}

#[cfg(test)]
mod tests {

    use std::iter::repeat_with;

    use fuel_core::{
        fuel_core_graphql_api::storage::transactions::{
            OwnedTransactionIndexKey,
            OwnedTransactions,
            TransactionStatuses,
        },
        producer::ports::BlockProducerDatabase,
    };
    use fuel_core_chain_config::{
        AddTable,
        AsTable,
        LastBlockConfig,
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
            Transactions,
        },
        transactional::AtomicView,
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
            Receipt,
            TransactionBuilder,
            TxPointer,
            UniqueIdentifier,
            UtxoId,
        },
        fuel_types::ChainId,
        services::txpool::TransactionStatus,
        tai64::Tai64,
    };
    use itertools::Itertools;
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
        db: CombinedDatabase,
        rng: StdRng,
    }

    #[derive(Debug, PartialEq)]
    struct SnapshotData {
        common: CommonData,
        transactions: Vec<TableEntry<Transactions>>,
        transaction_statuses: Vec<TableEntry<TransactionStatuses>>,
        owned_transactions: Vec<TableEntry<OwnedTransactions>>,
    }

    #[derive(Debug, PartialEq)]
    struct CommonData {
        coins: Vec<TableEntry<Coins>>,
        messages: Vec<TableEntry<Messages>>,
        contract_code: Vec<TableEntry<ContractsRawCode>>,
        contract_utxo: Vec<TableEntry<ContractsLatestUtxo>>,
        contract_state: Vec<TableEntry<ContractsState>>,
        contract_balance: Vec<TableEntry<ContractsAssets>>,
        block: TableEntry<FuelBlocks>,
    }

    impl CommonData {
        fn sorted(mut self) -> CommonData {
            self.coins.sort_by_key(|e| e.key);
            self.messages.sort_by_key(|e| e.key);
            self.contract_code.sort_by_key(|e| e.key);
            self.contract_utxo.sort_by_key(|e| e.key);
            self.contract_state.sort_by_key(|e| e.key);
            self.contract_balance.sort_by_key(|e| e.key);
            self
        }
    }

    impl SnapshotData {
        fn sorted(mut self) -> SnapshotData {
            self.common = self.common.sorted();
            self.transactions.sort_by_key(|e| e.key);
            self.transaction_statuses.sort_by_key(|e| e.key);
            self.owned_transactions.sort_by_key(|e| e.key.clone());
            self
        }

        fn into_state_config(self) -> StateConfig {
            let mut builder = StateConfigBuilder::default();
            builder.add(self.common.coins);
            builder.add(self.common.messages);
            builder.add(self.common.contract_code);
            builder.add(self.common.contract_utxo);
            builder.add(self.common.contract_state);
            builder.add(self.common.contract_balance);

            builder
                .build(Some(LastBlockConfig::from_header(
                    self.common.block.value.header(),
                    Default::default(),
                )))
                .unwrap()
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
                    .into_iter()
                    .flatten_ok()
                    .try_collect()
                    .unwrap()
            }

            let block = {
                let mut block = CompressedBlock::default();
                let last_block_config = reader
                    .last_block_config()
                    .cloned()
                    .expect("Expects the last block config to be set");
                block.header_mut().application_mut().da_height =
                    last_block_config.da_block_height;
                block
                    .header_mut()
                    .set_block_height(last_block_config.block_height);
                TableEntry {
                    key: last_block_config.block_height,
                    value: block,
                }
            };

            let reader = &mut reader;

            Self {
                common: CommonData {
                    coins: read(reader),
                    messages: read(reader),
                    contract_code: read(reader),
                    contract_utxo: read(reader),
                    contract_state: read(reader),
                    contract_balance: read(reader),
                    block,
                },
                transactions: read(reader),
                transaction_statuses: read(reader),
                owned_transactions: read(reader),
            }
        }
    }

    impl DbPopulator {
        fn new(db: CombinedDatabase, rng: StdRng) -> Self {
            Self { db, rng }
        }

        // Db will flush data upon being dropped. Important to do before snapshotting
        fn flush(self) {}

        fn given_persisted_data(&mut self) -> SnapshotData {
            let amount = 10;
            let coins = repeat_with(|| self.given_coin()).take(amount).collect();
            let messages = repeat_with(|| self.given_message()).take(amount).collect();

            let contract_ids = repeat_with(|| {
                let contract_id: ContractId = self.rng.gen();
                contract_id
            })
            .take(amount)
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
                        .take(amount)
                        .collect_vec()
                })
                .collect();

            let contract_balance = contract_ids
                .iter()
                .flat_map(|id| {
                    repeat_with(|| self.given_contract_asset(*id))
                        .take(amount)
                        .collect_vec()
                })
                .collect();

            let transactions = vec![self.given_transaction()];

            let transaction_statuses = vec![self.given_transaction_status()];

            let owned_transactions = vec![self.given_owned_transaction()];

            let block = self.given_block();

            SnapshotData {
                common: CommonData {
                    coins,
                    messages,
                    contract_code,
                    contract_utxo,
                    contract_state,
                    contract_balance,
                    block,
                },
                transactions,
                transaction_statuses,
                owned_transactions,
            }
        }

        fn given_transaction(&mut self) -> TableEntry<Transactions> {
            let tx = TransactionBuilder::script(
                self.generate_data(1000),
                self.generate_data(1000),
            )
            .finalize_as_transaction();

            let id = tx.id(&ChainId::new(self.rng.gen::<u64>()));

            self.db
                .on_chain_mut()
                .storage_as_mut::<Transactions>()
                .insert(&id, &tx)
                .unwrap();

            TableEntry { key: id, value: tx }
        }

        fn given_transaction_status(&mut self) -> TableEntry<TransactionStatuses> {
            let key = self.rng.gen();
            let status = TransactionStatus::Success {
                block_height: self.rng.gen(),
                time: Tai64(self.rng.gen::<u32>().into()),
                result: None,
                receipts: vec![Receipt::Return {
                    id: self.rng.gen(),
                    val: self.rng.gen(),
                    pc: self.rng.gen(),
                    is: self.rng.gen(),
                }],
                total_gas: self.rng.gen(),
                total_fee: self.rng.gen(),
            };

            self.db
                .off_chain_mut()
                .storage_as_mut::<TransactionStatuses>()
                .insert(&key, &status)
                .unwrap();

            TableEntry { key, value: status }
        }

        fn given_owned_transaction(&mut self) -> TableEntry<OwnedTransactions> {
            let key = OwnedTransactionIndexKey {
                owner: self.rng.gen(),
                block_height: self.rng.gen(),
                tx_idx: self.rng.gen(),
            };
            let value = self.rng.gen();

            self.db
                .off_chain_mut()
                .storage_as_mut::<OwnedTransactions>()
                .insert(&key, &value)
                .unwrap();

            TableEntry { key, value }
        }

        fn given_block(&mut self) -> TableEntry<FuelBlocks> {
            let mut block = CompressedBlock::default();
            let height = self.rng.gen();
            block.header_mut().application_mut().da_height = self.rng.gen();
            block.header_mut().set_block_height(height);
            let _ = self
                .db
                .on_chain_mut()
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
                .on_chain_mut()
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
                .on_chain_mut()
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
                .on_chain_mut()
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
                .on_chain_mut()
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
                .on_chain_mut()
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
                .on_chain_mut()
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
        std::fs::create_dir(&db_path)?;

        let mut db = DbPopulator::new(open_db(&db_path, None)?, StdRng::seed_from_u64(2));
        let state = db.given_persisted_data();
        db.flush();

        // when
        let fut = exec(Command {
            database_path: db_path,
            max_database_cache_size: DEFAULT_DATABASE_CACHE_SIZE,
            output_dir: snapshot_dir.clone(),
            subcommand: SubCommands::Everything {
                chain_config: None,
                encoding_command: Some(EncodingCommand::Encoding { encoding }),
            },
        });

        // Because the test_case macro doesn't work with async tests
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(fut)
            .unwrap();

        // then
        let snapshot = SnapshotMetadata::read(&snapshot_dir)?;

        let written_data = SnapshotData::read_from_snapshot(snapshot);

        assert_eq!(written_data.common.block, state.common.block);

        // Needed because of the way the test case macro works
        #[allow(irrefutable_let_patterns)]
        if let Encoding::Json = encoding {
            assert_ne!(written_data.common, state.common);
            assert_eq!(written_data.common, state.common.sorted());
        } else {
            assert_ne!(written_data, state);
            assert_eq!(written_data, state.sorted());
        }

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

        let state = db.given_persisted_data();
        db.flush();

        // when
        let fut = exec(Command {
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
        });

        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(fut)
            .unwrap();

        // then
        let snapshot = SnapshotMetadata::read(&snapshot_dir)?;
        let mut reader = SnapshotReader::open(snapshot)?;

        let expected_state = state.sorted();
        assert_groups_as_expected(group_size, expected_state.common.coins, &mut reader);

        Ok(())
    }

    #[tokio::test]
    async fn contract_snapshot_isolates_contract_correctly() -> anyhow::Result<()> {
        // given
        let temp_dir = tempfile::tempdir()?;
        let snapshot_dir = temp_dir.path().join("snapshot");

        let db_path = temp_dir.path().join("db");
        let mut db = DbPopulator::new(open_db(&db_path, None)?, StdRng::seed_from_u64(2));

        let original_state = db.given_persisted_data().sorted().into_state_config();

        let randomly_chosen_contract = original_state
            .contracts
            .choose(&mut db.rng)
            .unwrap()
            .clone();
        let contract_id = randomly_chosen_contract.contract_id;
        let mut latest_block = original_state.last_block.unwrap();
        latest_block.blocks_root = db
            .db
            .on_chain()
            .latest_view()
            .unwrap()
            .block_header_merkle_root(&latest_block.block_height)
            .unwrap();
        db.flush();

        // when
        exec(Command {
            database_path: db_path,
            output_dir: snapshot_dir.clone(),
            max_database_cache_size: DEFAULT_DATABASE_CACHE_SIZE,
            subcommand: SubCommands::Contract { contract_id },
        })
        .await?;

        // then
        let metadata = SnapshotMetadata::read(&snapshot_dir)?;
        let snapshot_state = StateConfig::from_snapshot_metadata(metadata)?;

        pretty_assertions::assert_eq!(
            snapshot_state,
            StateConfig {
                coins: vec![],
                messages: vec![],
                blobs: vec![],
                contracts: vec![randomly_chosen_contract],
                last_block: Some(latest_block),
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
        let actual: Vec<_> = reader.read().unwrap().into_iter().try_collect().unwrap();

        let expected = expected_data
            .into_iter()
            .chunks(expected_group_size)
            .into_iter()
            .map(|chunk| chunk.collect_vec())
            .collect_vec();

        assert_eq!(actual, expected);
    }
}
