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
    #[clap(long = "db-path", env = "SRS_DATABASE_PATH")]
    pub database_path: PathBuf,

    #[clap(
        name = "DB_TYPE",
        long = "db-type",
        default_value = "rocks-db",
        ignore_case = true,
        env
    )]
    pub database_type: DbType,

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
}

#[derive(
    Clone, Copy, Debug, Display, Eq, PartialEq, EnumString, EnumVariantNames, ValueEnum,
)]
#[strum(serialize_all = "kebab_case")]
pub enum DbType {
    InMemory,
    RocksDb,
}
