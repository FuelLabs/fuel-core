use crate::cli::DEFAULT_DB_PATH;
use anyhow::Context;
use clap::{
    Parser,
    Subcommand,
    ValueEnum,
};
use fuel_core::{
    chain_config::{
        ChainConfig,
        ChainStateDb,
        Encoder,
    },
    database::Database,
    types::fuel_types::ContractId,
};
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
    database_path: PathBuf,

    /// The sub-command of the snapshot operation.
    #[command(subcommand)]
    subcommand: SubCommands,
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum StateEncodingFormat {
    Json,
    Parquet,
}

#[derive(Debug, Clone, Subcommand)]
pub enum SubCommands {
    /// Creates a snapshot of the entire database and produces a chain config.
    #[command(arg_required_else_help = true)]
    Everything {
        /// Specify a path to the directory containing the chain config. Defaults used if no path
        /// is provided.
        #[clap(name = "CHAIN_CONFIG", long = "chain")]
        chain_config: Option<PathBuf>,
        /// Specify a path to an output directory for the chain config files.
        #[clap(name = "OUTPUT_DIR", long = "output-directory")]
        output_dir: PathBuf,
        /// State encoding format
        #[clap(name = "STATE_ENCODING_FORMAT", long = "state-encoding-format")]
        state_encoding_format: StateEncodingFormat,
    },
    /// Creates a config for the contract.
    #[command(arg_required_else_help = true)]
    Contract {
        /// The id of the contract to snapshot.
        #[clap(long = "id")]
        contract_id: ContractId,
    },
}

#[cfg(not(any(feature = "rocksdb", feature = "rocksdb-production")))]
pub async fn exec(command: Command) -> anyhow::Result<()> {
    Err(anyhow::anyhow!(
        "Rocksdb must be enabled to use the database at {}",
        command.database_path.display()
    ))
}

#[cfg(any(feature = "rocksdb", feature = "rocksdb-production"))]
pub fn exec(command: Command) -> anyhow::Result<()> {
    let db = open_db(&command.database_path)?;

    match command.subcommand {
        SubCommands::Everything {
            chain_config,
            output_dir,
            state_encoding_format,
        } => full_snapshot(chain_config, &output_dir, state_encoding_format, &db),
        SubCommands::Contract { contract_id } => contract_snapshot(&db, contract_id),
    }
}

fn contract_snapshot(
    db: impl ChainStateDb,
    contract_id: ContractId,
) -> Result<(), anyhow::Error> {
    let config = db.get_contract_config_by_id(contract_id)?;
    let stdout = std::io::stdout().lock();
    serde_json::to_writer_pretty(stdout, &config)
        .context("failed to dump contract snapshot to JSON")?;
    Ok(())
}

fn full_snapshot(
    chain_config: Option<PathBuf>,
    output_dir: &Path,
    state_encoding_format: StateEncodingFormat,
    db: impl ChainStateDb,
) -> Result<(), anyhow::Error> {
    let encoder = initialize_encoder(output_dir, state_encoding_format)?;
    encoding::write_chain_state(db, encoder)?;

    let chain_config = load_chain_config(chain_config)?;
    chain_config.create_config_file(output_dir)?;

    Ok(())
}

mod encoding {
    use fuel_core::chain_config::{
        ChainStateDb,
        Encoder,
        Group,
        WithId,
    };

    use fuel_core_storage::Result as StorageResult;
    use itertools::Itertools;

    pub(crate) fn write_chain_state(
        db: impl ChainStateDb,
        mut encoder: Encoder,
    ) -> anyhow::Result<()> {
        let group_size = 1000;

        let coins = db.iter_coin_configs();
        group_and_write(coins, group_size, |chunk| encoder.write_coins(chunk))?;

        let messages = db.iter_message_configs();
        group_and_write(messages, group_size, |chunk| encoder.write_messages(chunk))?;

        let contracts = db.iter_contract_configs();
        group_and_write(contracts, group_size, |chunk| {
            encoder.write_contracts(chunk)
        })?;

        group_by_contract_and_write(
            db.iter_contract_state_configs(),
            |state_group| encoder.write_contract_state(state_group),
            group_size,
        )?;

        group_by_contract_and_write(
            db.iter_contract_balance_configs(),
            |balance_group| encoder.write_contract_balance(balance_group),
            group_size,
        )?;

        encoder.close()?;

        Ok(())
    }

    fn group_and_write<T>(
        data: impl Iterator<Item = StorageResult<T>>,
        group_size: usize,
        mut write: impl FnMut(Vec<T>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        data.chunks(group_size)
            .into_iter()
            .try_for_each(|chunk| write(chunk.try_collect()?))
    }

    fn group_by_contract_and_write<T>(
        iter: impl Iterator<Item = StorageResult<WithId<T>>>,
        mut writer: impl FnMut(WithId<Group<T>>) -> anyhow::Result<()>,
        group_size: usize,
    ) -> anyhow::Result<()> {
        iter.process_results(move |iter| {
            iter.group_by(|e| e.id)
                .into_iter()
                .try_for_each(move |(key, group)| {
                    group
                        .map(|e| e.data)
                        .chunks(group_size)
                        .into_iter()
                        .map(move |chunk| WithId {
                            id: key,
                            data: chunk.collect_vec(),
                        })
                        .try_for_each(|el| writer(el))
                })
        })?
    }
}

fn initialize_encoder(
    output_dir: &Path,
    state_encoding_format: StateEncodingFormat,
) -> Result<Encoder, anyhow::Error> {
    std::fs::create_dir_all(output_dir)?;
    let encoder = match state_encoding_format {
        StateEncodingFormat::Json => Encoder::json(output_dir),
        StateEncodingFormat::Parquet => Encoder::parquet(output_dir, 1)?,
    };
    Ok(encoder)
}

fn load_chain_config(
    chain_config: Option<PathBuf>,
) -> Result<ChainConfig, anyhow::Error> {
    let chain_config = match chain_config {
        Some(dir) => ChainConfig::load_from_directory(dir)?,
        None => ChainConfig::local_testnet(),
    };

    Ok(chain_config)
}

fn open_db(path: &Path) -> anyhow::Result<impl ChainStateDb> {
    let data_source = fuel_core::state::rocks_db::RocksDb::default_open(path, None)
        .map_err(Into::<anyhow::Error>::into)
        .context(format!("failed to open database at path {path:?}",))?;
    Ok(Database::new(std::sync::Arc::new(data_source)))
}
