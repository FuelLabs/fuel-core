use crate::cli::DEFAULT_DB_PATH;
use clap::{
    Parser,
    Subcommand,
};
use fuel_core::types::fuel_types::ContractId;
use std::path::PathBuf;

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

#[derive(Debug, Clone, Subcommand)]
pub enum SubCommands {
    /// Creates a snapshot of the entire database and produces a chain config.
    #[command(arg_required_else_help = true)]
    Everything {
        /// Specify either an alias to a built-in configuration or filepath to a JSON file.
        #[clap(name = "CHAIN_CONFIG", long = "chain", default_value = "local_testnet")]
        chain_config: Option<String>,
        /// Specify a path to an output directory for the chain config files.
        #[clap(name = "OUTPUT_DIR", long = "output directory")]
        output_dir: PathBuf,
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
pub async fn exec(command: Command) -> anyhow::Result<()> {
    use anyhow::Context;
    use fuel_core::{
        chain_config::{
            ChainConfig,
            StateConfig,
            LOCAL_TESTNET,
        },
        database::Database,
    };
    let path = command.database_path;
    let data_source = fuel_core::state::rocks_db::RocksDb::default_open(&path, None)
        .map_err(Into::<anyhow::Error>::into)
        .context(format!(
            "failed to open database at path {}",
            path.display()
        ))?;
    let db = Database::new(std::sync::Arc::new(data_source));

    match command.subcommand {
        SubCommands::Everything {
            chain_config,
            output_dir,
        } => {
            let chain_conf = match chain_config.as_deref() {
                None | Some(LOCAL_TESTNET) => ChainConfig::local_testnet(),
                Some(path) => ChainConfig::load_from_directory(path)?,
            };
            let state_conf = StateConfig::generate_state_config(db)?;

            std::fs::create_dir_all(&output_dir)?;
            chain_conf.create_config_file(&output_dir)?;
            state_conf.create_config_file(&output_dir)?;
        }
        SubCommands::Contract { contract_id } => {
            let config = db.get_contract_config_by_id(contract_id)?;
            let stdout = std::io::stdout().lock();

            serde_json::to_writer_pretty(stdout, &config)
                .context("failed to dump contract snapshot to JSON")?;
        }
    }
    Ok(())
}
