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

/// SRS: The fuel state root service
#[derive(Debug, Clone, Parser)]
pub struct Args {
    #[clap(long, env = "SRS_DB_PATH")]
    pub db_path: PathBuf,

    #[clap(
        long,
        default_value = "rocks-db",
        ignore_case = true,
        env = "SRS_DB_TYPE"
    )]
    pub db_type: DbType,

    #[clap(long, default_value = "1024", env = "SRS_DB_MAX_FILES")]
    pub db_max_files: i32,

    #[clap(long, env = "SRS_DB_CACHE_CAPACITY")]
    pub db_cache_capacity: Option<usize>,

    #[clap(long, env = "SRS_FUEL_NODE_URL")]
    pub fuel_node_url: String,

    #[clap(long, default_value = "16", env = "SRS_BATCH_SIZE")]
    pub batch_size: u32,

    #[clap(long, default_value = "0", env = "SRS_CHAIN_ID")]
    pub chain_id: u64,

    #[clap(long, default_value = "localhost", env = "SRS_HOST")]
    pub host: String,

    #[clap(long, default_value = "8080", env = "SRS_PORT")]
    pub port: u16,

    #[clap(
        long,
        default_value = "human",
        ignore_case = true,
        env = "SRS_LOG_FORMAT"
    )]
    pub log_format: LogFormat,
}

#[derive(
    Clone, Copy, Debug, Display, Eq, PartialEq, EnumString, EnumVariantNames, ValueEnum,
)]
#[strum(serialize_all = "kebab_case")]
pub enum DbType {
    InMemory,
    RocksDb,
}

#[derive(
    Clone, Copy, Debug, Display, Eq, PartialEq, EnumString, EnumVariantNames, ValueEnum,
)]
#[strum(serialize_all = "kebab_case")]
pub enum LogFormat {
    JSON,
    Human,
}
