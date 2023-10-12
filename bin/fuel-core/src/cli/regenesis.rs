use crate::cli::DEFAULT_DB_PATH;
use fuel_core::types::fuel_types::ContractId;
use std::path::PathBuf;

/// Initialise the node state through regenesis
#[derive(Debug, Clone, Parser)]
pub struct Command {
    // TODO: can args be deduplicated (run command)?
    /// The path to the database.
    #[clap(
        name = "TEMP_DB_PATH",
        long = "temp-db-path",
        value_parser,
        default_value = (*DEFAULT_REGENESIS_DB_PATH).to_str().unwrap()
    )]
    database_path: PathBuf,

    #[clap(
        long = "db-type",
        default_value = "rocks-db",
        value_enum,
        ignore_case = true,
        env
    )]
    pub database_type: DbType,

    /// The maximum database cache size in bytes.
    #[arg(
        long = "max-database-cache-size",
        default_value_t = DEFAULT_DATABASE_CACHE_SIZE,
        env
    )]
    pub max_database_cache_size: usize,

    /// Specify a filepath to the snapshot folder.
    #[arg(
        name = "CONFIG_PATH",
        long = "snapshot path",
        //default_value = "",
        env
    )]
    pub config_path: String,
}

impl Command {
    pub fn get_config(self) -> DatabaseConfig {
        let Command {
            max_database_cache_size,
            database_path,
            database_type,
            ..
        } = self;

        Config {
            max_database_cache_size,
            database_path,
            database_type,
        }
    }
}

pub async fn exec(command: Command) -> anyhow::Result<()> {
    let config = command.get_config();
    let database = Database::from_config(config);

    let chain_conf = ChainConfig::load_from_file(&command.config_path)?;
    let state_importer = StateImporter::load_from_file(&command.config_path);

    database.init(&chain_conf)?;
    genesis::maybe_initialize_state(&config, &mut database)?;

    // TODO: drop original db and use temp

    Ok(())
}
