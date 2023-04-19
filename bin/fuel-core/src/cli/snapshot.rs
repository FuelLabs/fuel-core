use crate::cli::DEFAULT_DB_PATH;
use clap::Parser;
use std::path::PathBuf;

/// Print a snapshot of blockchain state to stdout.
#[derive(Debug, Clone, Parser)]
pub struct Command {
    #[clap(
        name = "DB_PATH",
        long = "db-path",
        value_parser,
        default_value = (*DEFAULT_DB_PATH).to_str().unwrap()
    )]
    pub database_path: PathBuf,

    /// Specify either an alias to a built-in configuration or filepath to a JSON file.
    #[clap(name = "CHAIN_CONFIG", long = "chain", default_value = "local_testnet")]
    pub chain_config: String,
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
    use crate::cli::init_logging;
    use anyhow::Context;
    use fuel_core::{
        chain_config::{
            ChainConfig,
            StateConfig,
        },
        database::Database,
    };
    init_logging("snapshot".to_string(), "local".to_string(), None).await?;
    let path = command.database_path;
    let config: ChainConfig = command.chain_config.parse()?;
    let data_source =
        fuel_core::state::rocks_db::RocksDb::default_open(&path, None).context(
            format!("failed to open database at path {}", path.display()),
        )?;
    let db = Database::new(std::sync::Arc::new(data_source));

    let state_conf = StateConfig::generate_state_config(db)?;

    let chain_conf = ChainConfig {
        initial_state: Some(state_conf),
        ..config
    };

    let stdout = std::io::stdout().lock();

    serde_json::to_writer(stdout, &chain_conf)
        .context("failed to dump snapshot to JSON")?;
    Ok(())
}
