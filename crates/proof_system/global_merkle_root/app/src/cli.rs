use std::path::PathBuf;

use clap::{
    Parser,
    ValueEnum,
};
use strum::{
    Display,
    EnumString,
    EnumVariantNames,
};

/// SRS: The fuel state root service.
#[derive(Debug, Clone, Parser)]
pub struct Args {
    /// Database path.
    #[clap(long, env = "SRS_DB_PATH")]
    pub db_path: PathBuf,

    /// Database type.
    #[clap(
        long,
        default_value = "rocks-db",
        ignore_case = true,
        env = "SRS_DB_TYPE"
    )]
    pub db_type: DbType,

    /// Max number of files used by RocksDB.
    #[clap(long, default_value = "1024", env = "SRS_DB_MAX_FILES")]
    pub db_max_files: i32,

    /// Database cache capacity.
    #[clap(long, env = "SRS_DB_CACHE_CAPACITY")]
    pub db_cache_capacity: Option<usize>,

    /// URL of the fuel node to fetch blocks from.
    #[clap(long, env = "SRS_FUEL_NODE_URL")]
    pub fuel_node_url: String,

    /// Batch size used when fetching chunks of blocks.
    #[clap(long, default_value = "16", env = "SRS_BATCH_SIZE")]
    pub batch_size: u32,

    /// Chain ID.
    #[clap(long, default_value = "0", env = "SRS_CHAIN_ID")]
    pub chain_id: u64,

    /// Host to serve state roots from.
    #[clap(long, default_value = "localhost", env = "SRS_HOST")]
    pub host: String,

    /// Port to serve state roots from.
    #[clap(long, default_value = "8080", env = "SRS_PORT")]
    pub port: u16,

    /// Format of logs.
    #[clap(
        long,
        default_value = "human",
        ignore_case = true,
        env = "SRS_LOG_FORMAT"
    )]
    pub log_format: LogFormat,
}

/// Database type.
#[derive(
    Clone, Copy, Debug, Display, Eq, PartialEq, EnumString, EnumVariantNames, ValueEnum,
)]
#[strum(serialize_all = "kebab_case")]
pub enum DbType {
    /// In memory.
    InMemory,
    /// RocksDB.
    RocksDb,
}

/// Log format.
#[derive(
    Clone, Copy, Debug, Display, Eq, PartialEq, EnumString, EnumVariantNames, ValueEnum,
)]
#[strum(serialize_all = "kebab_case")]
pub enum LogFormat {
    /// JSON logs.
    JSON,
    /// Human readable logs.
    Human,
}
