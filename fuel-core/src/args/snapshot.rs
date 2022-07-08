use crate::args::DEFAULT_DB_PATH;
#[cfg(not(feature = "rocksdb"))]
use anyhow::{anyhow, Error};
#[cfg(feature = "rocksdb")]
use anyhow::{Context, Error};
use clap::Parser;
#[cfg(feature = "rocksdb")]
use fuel_core::{
    config::chain_config::ChainConfig, config::chain_config::StateConfig, database::Database,
};
#[cfg(feature = "rocksdb")]
use std::io;
use std::path::PathBuf;

#[derive(clap::Subcommand, Clone, Debug)]
pub enum Snapshot {
    Snapshot(SnapshotCommand),
}

#[derive(Debug, Clone, Parser)]
pub struct SnapshotCommand {
    #[clap(
        name = "DB_PATH",
        long = "db-path",
        parse(from_os_str),
        default_value = (*DEFAULT_DB_PATH).to_str().unwrap()
    )]
    pub database_path: PathBuf,

    /// Specify either an alias to a built-in configuration or filepath to a JSON file.
    #[clap(name = "CHAIN_CONFIG", long = "chain", default_value = "local_testnet")]
    pub chain_config: String,
}

pub async fn exec(command: SnapshotCommand) -> anyhow::Result<(), Error> {
    #[cfg(not(feature = "rocksdb"))]
    {
        Err(anyhow!(
            "Rocksdb must be enabled to use the database at {}",
            command.database_path.display()
        ))
    }
    #[cfg(feature = "rocksdb")]
    {
        let path = command.database_path;
        let _config: ChainConfig = command.chain_config.parse()?;
        let db = Database::open(&path).context(format!(
            "failed to open database at path {}",
            path.display()
        ))?;

        let state_conf = StateConfig::generate_state_config(db)?;

        let chain_conf = ChainConfig {
            chain_name: _config.chain_name,
            block_production: _config.block_production,
            initial_state: Some(state_conf),
            transaction_parameters: _config.transaction_parameters,
        };

        let stdout = io::stdout().lock();

        serde_json::to_writer(stdout, &chain_conf).context("failed to dump snapshot to JSON")?;
        Ok(())
    }
}
