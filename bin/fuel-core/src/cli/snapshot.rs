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
        StateWriter,
    },
    database::Database,
    types::fuel_types::ContractId,
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
        /// Specify a a path to the directory containing the chain config. Defaults used if no path
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
    write_chain_state(&db, encoder)?;

    let height = db.get_block_height()?;
    let chain_config = ChainConfig {
        height: Some(height),
        ..load_chain_config(chain_config)?
    };
    chain_config.create_config_file(output_dir)?;

    Ok(())
}

fn write_chain_state(
    db: impl ChainStateDb,
    mut encoder: StateWriter,
) -> anyhow::Result<()> {
    fn write<T>(
        data: impl Iterator<Item = StorageResult<T>>,
        group_size: usize,
        mut write: impl FnMut(Vec<T>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        data.chunks(group_size)
            .into_iter()
            .try_for_each(|chunk| write(chunk.try_collect()?))
    }

    let group_size = 1000;

    let coins = db.iter_coin_configs();
    write(coins, group_size, |chunk| encoder.write_coins(chunk))?;

    let messages = db.iter_message_configs();
    write(messages, group_size, |chunk| encoder.write_messages(chunk))?;

    let contracts = db.iter_contract_configs();
    write(contracts, group_size, |chunk| {
        encoder.write_contracts(chunk)
    })?;

    let contract_states = db.iter_contract_state_configs();
    write(contract_states, group_size, |chunk| {
        encoder.write_contract_state(chunk)
    })?;

    let contract_balances = db.iter_contract_balance_configs();
    write(contract_balances, group_size, |chunk| {
        encoder.write_contract_balance(chunk)
    })?;

    encoder.close()?;

    Ok(())
}

fn initialize_encoder(
    output_dir: &Path,
    state_encoding_format: StateEncodingFormat,
) -> Result<StateWriter, anyhow::Error> {
    std::fs::create_dir_all(output_dir)?;
    let encoder = match state_encoding_format {
        StateEncodingFormat::Json => StateWriter::json(output_dir),
        StateEncodingFormat::Parquet => StateWriter::parquet(output_dir, 1)?,
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
