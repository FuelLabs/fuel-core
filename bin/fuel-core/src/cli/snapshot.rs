use crate::cli::DEFAULT_DB_PATH;
use anyhow::Context;
use clap::{
    Parser,
    Subcommand,
};
use fuel_core::{
    chain_config::{
        ChainConfig,
        ChainStateDb,
    },
    database::{
        database_description::on_chain::OnChain,
        Database,
    },
    types::fuel_types::ContractId,
};
use fuel_core_chain_config::{
    SnapshotMetadata,
    StateWriter,
    MAX_GROUP_SIZE,
};
use fuel_core_storage::Result as StorageResult;
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
        default_value = (*DEFAULT_DB_PATH).to_str().unwrap()
    )]
    pub(crate) database_path: PathBuf,

    /// Where to save the snapshot
    #[arg(name = "OUTPUT_DIR", long = "output-directory")]
    pub(crate) output_dir: PathBuf,

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
    let db = open_db(&command.database_path)?;
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
            full_snapshot(chain_config, &output_dir, encoding, &db)
        }
        SubCommands::Contract { contract_id } => {
            contract_snapshot(&db, contract_id, &output_dir)
        }
    }
}

fn contract_snapshot(
    db: impl ChainStateDb,
    contract_id: ContractId,
    output_dir: &Path,
) -> Result<(), anyhow::Error> {
    std::fs::create_dir_all(output_dir)?;

    let (contract, state, balance) = db.get_contract_by_id(contract_id)?;
    let block_height = db.get_block_height()?;

    let metadata = write_metadata(output_dir, Encoding::Json)?;
    let mut writer = StateWriter::for_snapshot(&metadata)?;

    writer.write_contracts(vec![contract])?;
    writer.write_contract_state(state)?;
    writer.write_contract_balance(balance)?;
    writer.write_block_height(block_height)?;
    writer.close()?;
    Ok(())
}

fn full_snapshot(
    prev_chain_config: Option<PathBuf>,
    output_dir: &Path,
    encoding: Encoding,
    db: impl ChainStateDb,
) -> Result<(), anyhow::Error> {
    std::fs::create_dir_all(output_dir)?;

    let metadata = write_metadata(output_dir, encoding)?;

    write_chain_state(&db, &metadata)?;
    write_chain_config(prev_chain_config, metadata.chain_config())?;
    Ok(())
}

fn write_chain_config(
    chain_config: Option<PathBuf>,
    file: &Path,
) -> Result<(), anyhow::Error> {
    let chain_config = load_chain_config(chain_config)?;

    chain_config.write(file)
}

fn write_metadata(dir: &Path, encoding: Encoding) -> anyhow::Result<SnapshotMetadata> {
    match encoding {
        Encoding::Json => SnapshotMetadata::write_json(dir),
        #[cfg(feature = "parquet")]
        Encoding::Parquet {
            compression,
            group_size,
            ..
        } => SnapshotMetadata::write_parquet(dir, compression.try_into()?, group_size),
    }
}

fn write_chain_state(
    db: impl ChainStateDb,
    metadata: &SnapshotMetadata,
) -> anyhow::Result<()> {
    let mut writer = StateWriter::for_snapshot(metadata)?;
    fn write<T>(
        data: impl Iterator<Item = StorageResult<T>>,
        group_size: usize,
        mut write: impl FnMut(Vec<T>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        data.chunks(group_size)
            .into_iter()
            .try_for_each(|chunk| write(chunk.try_collect()?))
    }
    let group_size = metadata
        .state_encoding()
        .group_size()
        .unwrap_or(MAX_GROUP_SIZE);

    let coins = db.iter_coin_configs();
    write(coins, group_size, |chunk| writer.write_coins(chunk))?;

    let messages = db.iter_message_configs();
    write(messages, group_size, |chunk| writer.write_messages(chunk))?;

    let contracts = db.iter_contract_configs();
    write(contracts, group_size, |chunk| writer.write_contracts(chunk))?;

    let contract_states = db.iter_contract_state_configs();
    write(contract_states, group_size, |chunk| {
        writer.write_contract_state(chunk)
    })?;

    let contract_balances = db.iter_contract_balance_configs();
    write(contract_balances, group_size, |chunk| {
        writer.write_contract_balance(chunk)
    })?;

    writer.write_block_height(db.get_block_height()?)?;

    writer.close()?;

    Ok(())
}

fn load_chain_config(
    chain_config: Option<PathBuf>,
) -> Result<ChainConfig, anyhow::Error> {
    let chain_config = match chain_config {
        Some(file) => ChainConfig::load(file)?,
        None => ChainConfig::local_testnet(),
    };

    Ok(chain_config)
}

fn open_db(path: &Path) -> anyhow::Result<Database> {
    Database::<OnChain>::open(path, None)
        .map_err(Into::<anyhow::Error>::into)
        .context(format!("failed to open database at path {path:?}",))
}

#[cfg(test)]
mod tests {

    use std::iter::repeat_with;

    use fuel_core::database::{
        database_description::{
            on_chain::OnChain,
            DatabaseDescription,
            DatabaseMetadata,
        },
        metadata::MetadataTable,
    };
    use fuel_core_chain_config::{
        CoinConfig,
        ContractBalanceConfig,
        ContractConfig,
        ContractStateConfig,
        MessageConfig,
        StateConfig,
    };
    use fuel_core_storage::{
        tables::{
            Coins,
            ContractsAssets,
            ContractsInfo,
            ContractsLatestUtxo,
            ContractsRawCode,
            ContractsState,
            Messages,
        },
        ContractsAssetKey,
        ContractsStateKey,
        StorageAsMut,
    };
    use fuel_core_types::{
        blockchain::primitives::DaBlockHeight,
        entities::{
            coins::coin::{
                CompressedCoin,
                CompressedCoinV1,
            },
            contract::{
                ContractUtxoInfo,
                ContractsInfoType,
            },
            message::{
                Message,
                MessageV1,
            },
        },
        fuel_tx::{
            TxPointer,
            UtxoId,
        },
        fuel_types::BlockHeight,
        fuel_vm::Salt,
    };
    use rand::{
        rngs::StdRng,
        seq::SliceRandom,
        Rng,
        SeedableRng,
    };
    use test_case::test_case;

    use super::*;

    struct DbPopulator {
        db: Database,
        rng: StdRng,
    }

    impl DbPopulator {
        fn new(db: Database, rng: StdRng) -> Self {
            Self { db, rng }
        }

        fn given_persisted_state(
            &mut self,
            coins: usize,
            messages: usize,
            contracts: usize,
            states_per_contract: usize,
            balances_per_contract: usize,
        ) -> StateConfig {
            let coins = repeat_with(|| self.given_coin()).take(coins).collect();

            let messages = repeat_with(|| self.given_message())
                .take(messages)
                .collect();

            let contracts: Vec<ContractConfig> = repeat_with(|| self.given_contract())
                .take(contracts)
                .collect();

            let contract_state = {
                let ids = contracts.iter().map(|contract| contract.contract_id);
                (0..states_per_contract)
                    .cartesian_product(ids)
                    .map(|(_, id)| self.given_contract_state(id))
                    .collect()
            };

            let contract_balance = {
                let ids = contracts.iter().map(|contract| contract.contract_id);
                (0..balances_per_contract)
                    .cartesian_product(ids)
                    .map(|(_, id)| self.given_contract_balance(id))
                    .collect()
            };

            let block_height = self.given_block_height();

            StateConfig {
                coins,
                messages,
                contracts,
                contract_state,
                contract_balance,
                block_height,
            }
        }

        fn given_block_height(&mut self) -> BlockHeight {
            let height = BlockHeight::from(10);
            self.db
                .storage::<MetadataTable<OnChain>>()
                .insert(
                    &(),
                    &DatabaseMetadata::V1 {
                        version: OnChain::version(),
                        height,
                    },
                )
                .unwrap();

            height
        }

        fn given_coin(&mut self) -> CoinConfig {
            let tx_id = self.rng.gen();
            let output_index = self.rng.gen();
            let coin = CompressedCoin::V1(CompressedCoinV1 {
                owner: self.rng.gen(),
                amount: self.rng.gen(),
                asset_id: self.rng.gen(),
                tx_pointer: self.rng.gen(),
            });
            self.db
                .storage_as_mut::<Coins>()
                .insert(&UtxoId::new(tx_id, output_index), &coin)
                .unwrap();

            CoinConfig {
                tx_id,
                output_index,
                tx_pointer_block_height: coin.tx_pointer().block_height(),
                tx_pointer_tx_idx: coin.tx_pointer().tx_index(),
                owner: *coin.owner(),
                amount: *coin.amount(),
                asset_id: *coin.asset_id(),
            }
        }

        fn given_message(&mut self) -> MessageConfig {
            let message = Message::V1(MessageV1 {
                sender: self.rng.gen(),
                recipient: self.rng.gen(),
                amount: self.rng.gen(),
                nonce: self.rng.gen(),
                data: self.generate_data(100),
                da_height: DaBlockHeight(self.rng.gen()),
            });

            self.db
                .storage_as_mut::<Messages>()
                .insert(message.nonce(), &message)
                .unwrap();

            MessageConfig {
                sender: *message.sender(),
                recipient: *message.recipient(),
                nonce: *message.nonce(),
                amount: message.amount(),
                data: message.data().clone(),
                da_height: message.da_height(),
            }
        }

        fn given_contract(&mut self) -> ContractConfig {
            let contract_id = self.rng.gen();

            let code = self.generate_data(1000);
            self.db
                .storage_as_mut::<ContractsRawCode>()
                .insert(&contract_id, code.as_ref())
                .unwrap();

            let salt: Salt = self.rng.gen();
            self.db
                .storage_as_mut::<ContractsInfo>()
                .insert(&contract_id, &ContractsInfoType::V1(salt.into()))
                .unwrap();

            let utxo_id = UtxoId::new(self.rng.gen(), self.rng.gen());
            let tx_pointer = TxPointer::new(self.rng.gen(), self.rng.gen());

            self.db
                .storage::<ContractsLatestUtxo>()
                .insert(
                    &contract_id,
                    &ContractUtxoInfo::V1((utxo_id, tx_pointer).into()),
                )
                .unwrap();

            ContractConfig {
                contract_id,
                code,
                salt,
                tx_id: *utxo_id.tx_id(),
                output_index: utxo_id.output_index(),
                tx_pointer_block_height: tx_pointer.block_height(),
                tx_pointer_tx_idx: tx_pointer.tx_index(),
            }
        }

        fn generate_data(&mut self, max_amount: usize) -> Vec<u8> {
            let mut data = vec![0u8; self.rng.gen_range(0..=max_amount)];
            self.rng.fill(data.as_mut_slice());
            data
        }

        fn given_contract_state(
            &mut self,
            contract_id: ContractId,
        ) -> ContractStateConfig {
            let state_key = self.rng.gen();
            let key = ContractsStateKey::new(&contract_id, &state_key);
            let value = self.generate_data(100);
            self.db
                .storage_as_mut::<ContractsState>()
                .insert(&key, &value)
                .unwrap();

            ContractStateConfig {
                contract_id,
                key: state_key,
                value,
            }
        }

        fn given_contract_balance(
            &mut self,
            contract_id: ContractId,
        ) -> ContractBalanceConfig {
            let asset_id = self.rng.gen();
            let key = ContractsAssetKey::new(&contract_id, &asset_id);
            let amount = self.rng.gen();

            self.db
                .storage_as_mut::<ContractsAssets>()
                .insert(&key, &amount)
                .unwrap();

            ContractBalanceConfig {
                contract_id,
                asset_id,
                amount,
            }
        }
    }

    #[cfg_attr(feature = "parquet", test_case(Encoding::Parquet { group_size: 2, compression: 1 }; "parquet"))]
    #[test_case(Encoding::Json; "json")]
    fn everything_snapshot_correct_and_sorted(encoding: Encoding) -> anyhow::Result<()> {
        // given
        let temp_dir = tempfile::tempdir()?;

        let snapshot_dir = temp_dir.path().join("snapshot");
        let db_path = temp_dir.path().join("db");
        let mut db = DbPopulator::new(open_db(&db_path)?, StdRng::seed_from_u64(2));

        let height = db.given_block_height();
        let state = db.given_persisted_state(10, 10, 10, 10, 10);
        drop(db);

        // when
        exec(Command {
            database_path: db_path,
            output_dir: snapshot_dir.clone(),
            subcommand: SubCommands::Everything {
                chain_config: None,
                encoding_command: Some(EncodingCommand::Encoding { encoding }),
            },
        })?;

        // then
        let snapshot = SnapshotMetadata::read(&snapshot_dir)?;

        let snapshot = StateConfig::from_snapshot_metadata(snapshot)?;

        assert_eq!(snapshot.block_height, height);

        assert_ne!(snapshot, state);
        assert_eq!(snapshot, sorted_state(state));

        Ok(())
    }

    #[cfg(feature = "parquet")]
    #[test_case(2; "parquet group_size=2")]
    #[test_case(5; "parquet group_size=5")]
    fn everything_snapshot_respects_group_size(group_size: usize) -> anyhow::Result<()> {
        use fuel_core_chain_config::{
            ParquetFiles,
            StateReader,
        };

        // given
        let temp_dir = tempfile::tempdir()?;

        let snapshot_dir = temp_dir.path().join("snapshot");
        let db_path = temp_dir.path().join("db");
        let mut db = DbPopulator::new(open_db(&db_path)?, StdRng::seed_from_u64(2));

        db.given_block_height();
        let state = db.given_persisted_state(10, 10, 10, 10, 10);
        drop(db);

        // when
        exec(Command {
            database_path: db_path,
            output_dir: snapshot_dir.clone(),
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
        let files = ParquetFiles::snapshot_default(&snapshot_dir);
        let reader = StateReader::parquet(files)?;

        let expected_state = sorted_state(state);

        assert_groups_as_expected(group_size, expected_state.coins, || {
            reader.coins().unwrap()
        });
        assert_groups_as_expected(group_size, expected_state.messages, || {
            reader.messages().unwrap()
        });

        assert_groups_as_expected(group_size, expected_state.contracts, || {
            reader.contracts().unwrap()
        });

        assert_groups_as_expected(group_size, expected_state.contract_state, || {
            reader.contract_state().unwrap()
        });

        assert_groups_as_expected(group_size, expected_state.contract_balance, || {
            reader.contract_balance().unwrap()
        });

        Ok(())
    }

    #[test]
    fn contract_snapshot_isolates_contract_correctly() -> anyhow::Result<()> {
        // given
        let temp_dir = tempfile::tempdir()?;
        let snapshot_dir = temp_dir.path().join("snapshot");

        let db_path = temp_dir.path().join("db");
        let mut db = DbPopulator::new(open_db(&db_path)?, StdRng::seed_from_u64(2));

        let state = sorted_state(db.given_persisted_state(10, 10, 10, 10, 10));
        let random_contract = state.contracts.choose(&mut db.rng).unwrap().clone();
        let contract_id = random_contract.contract_id;
        drop(db);

        // when
        exec(Command {
            database_path: db_path,
            output_dir: snapshot_dir.clone(),
            subcommand: SubCommands::Contract { contract_id },
        })?;

        // then
        let metadata = SnapshotMetadata::read(&snapshot_dir)?;
        let snapshot_state = StateConfig::from_snapshot_metadata(metadata)?;

        let expected_contract_state = state
            .contract_state
            .iter()
            .filter(|state| state.contract_id == contract_id)
            .cloned()
            .collect_vec();

        let expected_contract_balance = state
            .contract_balance
            .iter()
            .filter(|balance| balance.contract_id == contract_id)
            .cloned()
            .collect_vec();

        assert_eq!(
            snapshot_state,
            StateConfig {
                coins: vec![],
                messages: vec![],
                contract_state: expected_contract_state,
                contract_balance: expected_contract_balance,
                contracts: vec![random_contract],
                block_height: state.block_height
            }
        );

        Ok(())
    }

    #[cfg(feature = "parquet")]
    fn assert_groups_as_expected<T, I>(
        group_size: usize,
        expected_data: Vec<T>,
        actual_data: impl FnOnce() -> I,
    ) where
        T: PartialEq + std::fmt::Debug,
        I: Iterator<Item = anyhow::Result<fuel_core_chain_config::Group<T>>>,
    {
        let expected = expected_data
            .into_iter()
            .chunks(group_size)
            .into_iter()
            .map(|chunk| chunk.collect_vec())
            .collect_vec();

        let actual = actual_data().map(|group| group.unwrap().data).collect_vec();

        assert_eq!(actual, expected);
    }

    fn sorted_state(mut state: StateConfig) -> StateConfig {
        state
            .coins
            .sort_by_key(|coin| UtxoId::new(coin.tx_id, coin.output_index));
        state.messages.sort_by_key(|msg| msg.nonce);
        state.contracts.sort_by_key(|contract| contract.contract_id);
        state
            .contract_state
            .sort_by_key(|state| (state.contract_id, state.key));
        state
            .contract_balance
            .sort_by_key(|balance| (balance.contract_id, balance.asset_id));
        state
    }
}
