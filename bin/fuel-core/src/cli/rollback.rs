use crate::cli::default_db_path;
use anyhow::Context;
use clap::Parser;
use fuel_core::{
    combined_database::CombinedDatabase,
    state::{
        historical_rocksdb::StateRewindPolicy,
        rocks_db::{
            ColumnsPolicy,
            DatabaseConfig,
        },
    },
};
use rlimit::{
    Resource,
    getrlimit,
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

    /// Defines a specific number of file descriptors that RocksDB can use.
    ///
    /// If defined as -1 no limit will be applied and will use the OS limits.
    /// If not defined the system default divided by two is used.
    #[clap(
        long = "rocksdb-max-fds",
        env,
        default_value = get_default_max_fds().to_string()
    )]
    pub rocksdb_max_fds: i32,

    /// The path to the database.
    #[clap(long = "target-block-height")]
    pub target_block_height: Option<u32>,

    /// The path to the database.
    #[clap(long = "target-da-block-height")]
    pub target_da_block_height: Option<u64>,
}

fn get_default_max_fds() -> i32 {
    getrlimit(Resource::NOFILE)
        .map(|(_, hard)| i32::try_from(hard.saturating_div(2)).unwrap_or(i32::MAX))
        .expect("Our supported platforms should return max FD.")
}

pub async fn exec(command: Command) -> anyhow::Result<()> {
    use crate::cli::ShutdownListener;

    let path = command.database_path.as_path();
    let db = CombinedDatabase::open(
        path,
        StateRewindPolicy::RewindFullRange,
        DatabaseConfig {
            cache_capacity: Some(64 * 1024 * 1024),
            max_fds: command.rocksdb_max_fds,
            columns_policy: ColumnsPolicy::Lazy,
        },
    )
    .map_err(Into::<anyhow::Error>::into)
    .context(format!("failed to open combined database at path {path:?}"))?;

    let mut shutdown_listener = ShutdownListener::spawn();
    if let Some(target_block_height) = command.target_block_height {
        db.rollback_to(target_block_height.into(), &mut shutdown_listener)?;
    }

    if let Some(target_da_height) = command.target_da_block_height {
        db.rollback_relayer_to(target_da_height.into(), &mut shutdown_listener)?;
    }

    Ok(())
}
