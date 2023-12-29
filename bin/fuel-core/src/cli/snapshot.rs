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
        StateWriter,
    },
    database::Database,
    types::fuel_types::ContractId,
};
use fuel_core_chain_config::MAX_GROUP_SIZE;
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

impl Encoding {
    fn group_size(self) -> usize {
        match self {
            Encoding::Json => MAX_GROUP_SIZE,
            #[cfg(feature = "parquet")]
            Encoding::Parquet { group_size, .. } => group_size,
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
        /// Specify a path to the directory containing the chain config. Defaults used if no path
        /// is provided.
        #[arg(name = "CHAIN_CONFIG", long = "chain")]
        chain_config: Option<PathBuf>,
        /// Encoding format for the chain state files.
        #[clap(subcommand)]
        encoding_command: Option<EncodingCommand>,
    },
    /// Creates a config for the contract.
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
    let (contract, state, balance) = db.get_contract_by_id(contract_id)?;
    let mut writer = initialize_state_writer(output_dir, Encoding::Json)?;

    writer.write_contracts(vec![contract])?;
    writer.write_contract_state(state)?;
    writer.write_contract_balance(balance)?;
    writer.close()?;

    Ok(())
}

fn full_snapshot(
    chain_config: Option<PathBuf>,
    output_dir: &Path,
    encoding: Encoding,
    db: impl ChainStateDb,
) -> Result<(), anyhow::Error> {
    let encoder = initialize_state_writer(output_dir, encoding)?;
    write_chain_state(&db, encoder, encoding.group_size())?;

    let height = db.get_block_height()?;
    let chain_config = ChainConfig {
        height: Some(height),
        ..load_chain_config(chain_config)?
    };
    chain_config.create_config_file(output_dir)?;

    Ok(())
}

// TODO: segfault rename this to state writer
fn write_chain_state(
    db: impl ChainStateDb,
    mut encoder: StateWriter,
    group_size: usize,
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

fn initialize_state_writer(
    output_dir: &Path,
    encoding: Encoding,
) -> Result<StateWriter, anyhow::Error> {
    std::fs::create_dir_all(output_dir)?;
    let encoder = match encoding {
        Encoding::Json => StateWriter::json(output_dir),
        #[cfg(feature = "parquet")]
        Encoding::Parquet { compression, .. } => StateWriter::parquet(
            output_dir,
            fuel_core_chain_config::ZstdCompressionLevel::try_from(compression)?,
        )?,
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
