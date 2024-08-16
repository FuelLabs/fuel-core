use crate::cli::default_db_path;
use anyhow::Context;
use clap::Parser;
use fuel_core::{
    combined_database::CombinedDatabase,
    state::historical_rocksdb::StateRewindPolicy,
};
use std::path::PathBuf;

/// Rollbacks the state of the blockchain to a specific block height.
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

    /// The path to the database.
    #[clap(long = "target-block-height")]
    pub target_block_height: u32,
}

pub async fn exec(command: Command) -> anyhow::Result<()> {
    use crate::cli::ShutdownListener;

    let path = command.database_path.as_path();
    let db = CombinedDatabase::open(
        path,
        64 * 1024 * 1024,
        StateRewindPolicy::RewindFullRange,
    )
    .map_err(Into::<anyhow::Error>::into)
    .context(format!("failed to open combined database at path {path:?}"))?;

    let mut shutdown_listener = ShutdownListener::spawn();
    let target_block_height = command.target_block_height.into();

    db.rollback_to(target_block_height, &mut shutdown_listener)?;

    Ok(())
}
