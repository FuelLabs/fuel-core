use clap::Parser;
use fuel_core::ShutdownListener;
use fuel_core_chain_config::{
    ChainConfig,
    SnapshotReader,
    StateConfig,
};
use std::{
    env,
    path::PathBuf,
    str::FromStr,
};
use tracing_subscriber::{
    filter::EnvFilter,
    layer::SubscriberExt,
    registry,
    Layer,
};

#[cfg(feature = "env")]
use dotenvy::dotenv;

pub fn default_db_path() -> PathBuf {
    dirs::home_dir().unwrap().join(".fuel").join("db")
}

pub mod fee_contract;
#[cfg(feature = "rocksdb")]
pub mod rollback;
pub mod run;
#[cfg(feature = "rocksdb")]
pub mod snapshot;

// Default database cache is 1 GB
pub const DEFAULT_DATABASE_CACHE_SIZE: usize = 1024 * 1024 * 1024;

#[derive(Parser, Debug)]
#[clap(
    name = "fuel-core",
    about = "Fuel client implementation",
    version,
    rename_all = "kebab-case"
)]
pub struct Opt {
    #[clap(subcommand)]
    command: Fuel,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Parser)]
pub enum Fuel {
    Run(run::Command),
    #[cfg(feature = "rocksdb")]
    Snapshot(snapshot::Command),
    #[cfg(feature = "rocksdb")]
    Rollback(rollback::Command),
    GenerateFeeContract(fee_contract::Command),
}

pub const LOG_FILTER: &str = "RUST_LOG";
pub const HUMAN_LOGGING: &str = "HUMAN_LOGGING";

#[cfg(feature = "env")]
fn init_environment() -> Option<PathBuf> {
    dotenv().ok()
}

#[cfg(not(feature = "env"))]
fn init_environment() -> Option<PathBuf> {
    None
}

pub fn init_logging() {
    let filter = match env::var_os(LOG_FILTER) {
        Some(_) => {
            EnvFilter::try_from_default_env().expect("Invalid `RUST_LOG` provided")
        }
        None => EnvFilter::new("info"),
    };

    let human_logging = env::var_os(HUMAN_LOGGING)
        .map(|s| {
            bool::from_str(s.to_str().unwrap())
                .expect("Expected `true` or `false` to be provided for `HUMAN_LOGGING`")
        })
        .unwrap_or(true);

    let layer = tracing_subscriber::fmt::Layer::default().with_writer(std::io::stderr);

    let fmt = if human_logging {
        // use pretty logs
        layer
            .with_ansi(true)
            .with_level(true)
            .with_line_number(true)
            .boxed()
    } else {
        // use machine parseable structured logs
        layer
            // disable terminal colors
            .with_ansi(false)
            .with_level(true)
            .with_line_number(true)
            // use json
            .json()
            .boxed()
    };

    let subscriber = registry::Registry::default() // provide underlying span data store
        .with(filter) // filter out low-level debug tracing (eg tokio executor)
        .with(fmt); // log to stdout

    tracing::subscriber::set_global_default(subscriber)
        .expect("setting global default failed");
}

pub async fn run_cli() -> anyhow::Result<()> {
    init_logging();
    if let Some(path) = init_environment() {
        let path = path.display();
        tracing::info!("Loading environment variables from {path}");
    }
    let opt = Opt::try_parse();
    if opt.is_err() {
        let command = run::Command::try_parse();
        if let Ok(command) = command {
            tracing::warn!("This cli format for running `fuel-core` is deprecated and will be removed. Please use `fuel-core run` or use `--help` for more information");
            return run::exec(command).await;
        }
    }

    match opt {
        Ok(opt) => match opt.command {
            Fuel::Run(command) => run::exec(command).await,
            #[cfg(feature = "rocksdb")]
            Fuel::Snapshot(command) => snapshot::exec(command).await,
            Fuel::GenerateFeeContract(command) => fee_contract::exec(command).await,
            Fuel::Rollback(command) => rollback::exec(command).await,
        },
        Err(e) => {
            // Prints the error and exits.
            e.exit()
        }
    }
}

/// Returns the chain configuration for the local testnet.
pub fn local_testnet_chain_config() -> ChainConfig {
    const TESTNET_CHAIN_CONFIG: &[u8] =
        include_bytes!("../chainspec/local-testnet/chain_config.json");
    const TESTNET_CHAIN_CONFIG_STATE_BYTECODE: &[u8] =
        include_bytes!("../chainspec/local-testnet/state_transition_bytecode.wasm");

    let mut config: ChainConfig = serde_json::from_slice(TESTNET_CHAIN_CONFIG).unwrap();
    config.state_transition_bytecode = TESTNET_CHAIN_CONFIG_STATE_BYTECODE.to_vec();
    config
}

/// Returns the chain configuration for the local testnet.
pub fn local_testnet_reader() -> SnapshotReader {
    const TESTNET_STATE_CONFIG: &[u8] =
        include_bytes!("../chainspec/local-testnet/state_config.json");

    let state_config: StateConfig = serde_json::from_slice(TESTNET_STATE_CONFIG).unwrap();

    SnapshotReader::new_in_memory(local_testnet_chain_config(), state_config)
}

#[cfg(feature = "rocksdb")]
#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use clap::Parser;
    use fuel_core_types::fuel_types::ContractId;
    use std::path::PathBuf;

    use crate::cli::{
        snapshot,
        Fuel,
    };

    use super::Opt;

    fn parse_cli(line: &str, suffix: &str) -> anyhow::Result<Opt> {
        let words = line
            .split_ascii_whitespace()
            .chain(suffix.split_ascii_whitespace());
        Opt::try_parse_from(words).map_err(|e| anyhow!(e.to_string()))
    }

    mod snapshot_tests {
        use crate::cli::default_db_path;

        use super::*;

        #[test]
        fn can_snapshot() {
            // given
            let line = "./core snapshot";
            let irrelevant_remainder = "--output-directory dir everything  encoding json";

            // when
            let command = parse_cli(line, irrelevant_remainder)
                .expect("should parse the snapshot command")
                .command;

            // then
            assert!(matches!(command, Fuel::Snapshot(_)));
        }

        #[test]
        fn db_is_default_if_not_given() {
            // given
            let line = "./core snapshot";
            let irrelevant_remainder = "--output-directory dir everything encoding json";

            // when
            let command = parse_cli(line, irrelevant_remainder)
                .expect("should parse the snapshot command")
                .command;

            // then
            let Fuel::Snapshot(snapshot::Command { database_path, .. }) = command else {
                panic!("Expected a snapshot command")
            };
            assert_eq!(database_path, default_db_path().as_path());
        }

        #[test]
        fn db_is_as_given() {
            // given
            let line = "./core snapshot --db-path ./some/path";
            let irrelevant_remainder = "--output-directory dir everything  encoding json";

            // when
            let command = parse_cli(line, irrelevant_remainder)
                .expect("should parse the snapshot command")
                .command;

            // then
            let Fuel::Snapshot(snapshot::Command { database_path, .. }) = command else {
                panic!("Expected a snapshot command")
            };
            assert_eq!(database_path, PathBuf::from("./some/path"));
        }

        #[test]
        fn output_dir_required() {
            // given
            let line = "./core snapshot";
            let irrelevant_remainder = "everything encoding json";

            // when
            let result = parse_cli(line, irrelevant_remainder);

            // then
            assert!(result.is_err());
        }

        #[test]
        fn output_dir_is_as_given() {
            // given
            let line = "./core snapshot --output-directory ./some/path";
            let irrelevant_remainder = "everything encoding json";

            // when
            let command = parse_cli(line, irrelevant_remainder)
                .expect("should parse the snapshot command")
                .command;

            // then
            let Fuel::Snapshot(snapshot::Command { output_dir, .. }) = command else {
                panic!("Expected a snapshot command")
            };
            assert_eq!(output_dir, PathBuf::from("./some/path"));
        }
    }

    mod snapshot_everything_tests {
        use anyhow::bail;

        use super::*;

        fn extract_everything_command(
            command: Fuel,
        ) -> anyhow::Result<(Option<PathBuf>, PathBuf, Option<snapshot::EncodingCommand>)>
        {
            match command {
                Fuel::Snapshot(snapshot::Command {
                    subcommand:
                        snapshot::SubCommands::Everything {
                            chain_config,
                            encoding_command,
                        },
                    output_dir,
                    ..
                }) => Ok((chain_config, output_dir, encoding_command)),
                _ => bail!("Expected a snapshot everything command"),
            }
        }

        #[test]
        fn snapshot_everything() {
            // given
            let line = "./core snapshot --output-directory dir everything ";
            let irrelevant_remainder = "encoding json";

            // when
            let command = parse_cli(line, irrelevant_remainder)
                .expect("should parse the snapshot command")
                .command;

            // then
            extract_everything_command(command).expect("Can extract command");
        }

        #[test]
        fn output_dir_required() {
            // given
            let line = "./core snapshot everything";

            // when
            let result = parse_cli(line, "");

            // then
            assert!(result.is_err());
        }

        #[test]
        fn chain_config_is_as_given() {
            // given
            let line =
                "./core snapshot --output-directory ./some/path everything --chain ./some/chain/config";

            // when
            let command = parse_cli(line, "")
                .expect("should parse the snapshot command")
                .command;

            // then
            let (chain_config, _, _) =
                extract_everything_command(command).expect("Can extract command");
            assert_eq!(chain_config, Some(PathBuf::from("./some/chain/config")));
        }

        #[test]
        fn encoding_is_optional() {
            // given
            let line = "./core snapshot --output-directory ./some/path everything";
            // when
            let command = parse_cli(line, "")
                .expect("should parse the snapshot command")
                .command;

            // then
            let (_, _, encoding_command) =
                extract_everything_command(command).expect("Can extract command");
            assert!(encoding_command.is_none());
        }

        #[test]
        fn chain_config_dir_is_optional() {
            // given
            let line =
                "./core snapshot --output-directory ./some/path everything encoding json";

            // when
            let command = parse_cli(line, "")
                .expect("should parse the snapshot command")
                .command;

            // then
            let (chain_config, _, _) =
                extract_everything_command(command).expect("Can extract command");
            assert!(chain_config.is_none());
        }

        #[test]
        fn can_choose_json_encoding() {
            // given
            let line = "./core snapshot --output-directory dir everything encoding json";

            // when
            let command = parse_cli(line, "")
                .expect("should parse the snapshot command")
                .command;

            // then
            let (_, _, encoding_command) =
                extract_everything_command(command).expect("Can extract command");

            let Some(snapshot::EncodingCommand::Encoding {
                encoding: snapshot::Encoding::Json,
            }) = encoding_command
            else {
                panic!("Expected a snapshot everything command with json encoding");
            };
        }

        #[cfg(feature = "parquet")]
        #[test]
        fn can_choose_parquet_encoding() {
            // given
            let line =
                "./core snapshot --output-directory dir everything encoding parquet";

            // when
            let command = parse_cli(line, "")
                .expect("should parse the snapshot command")
                .command;

            // then
            let (_, _, encoding_command) =
                extract_everything_command(command).expect("Can extract command");

            let Some(snapshot::EncodingCommand::Encoding {
                encoding: snapshot::Encoding::Parquet { .. },
            }) = encoding_command
            else {
                panic!("Expected a snapshot everything command with parquet encoding");
            };
        }

        #[cfg(feature = "parquet")]
        #[test]
        fn group_size_is_configurable() {
            // given
            let line =
                "./core snapshot --output-directory dir everything encoding parquet --group-size 101";

            // when
            let command = parse_cli(line, "")
                .expect("should parse the snapshot command")
                .command;

            // then
            let (_, _, encoding_command) =
                extract_everything_command(command).expect("Can extract command");

            let Some(snapshot::EncodingCommand::Encoding {
                encoding: snapshot::Encoding::Parquet { group_size, .. },
            }) = encoding_command
            else {
                panic!("Expected a snapshot everything command with parquet encoding");
            };

            assert_eq!(group_size, 101);
        }

        #[cfg(feature = "parquet")]
        #[test]
        fn group_size_has_a_default() {
            // given
            let line =
                "./core snapshot --output-directory dir everything encoding parquet";

            // when
            let command = parse_cli(line, "")
                .expect("should parse the snapshot command")
                .command;

            // then
            let (_, _, encoding_command) =
                extract_everything_command(command).expect("Can extract command");

            let Some(snapshot::EncodingCommand::Encoding {
                encoding: snapshot::Encoding::Parquet { group_size, .. },
            }) = encoding_command
            else {
                panic!("Expected a snapshot everything command with parquet encoding");
            };

            assert_eq!(group_size, 10000);
        }

        #[cfg(feature = "parquet")]
        #[test]
        fn can_configure_compression() {
            // given
            let line =
                "./core snapshot --output-directory dir everything encoding parquet --compression-level 7";

            // when
            let command = parse_cli(line, "")
                .expect("should parse the snapshot command")
                .command;

            // then
            let (_, _, encoding_command) =
                extract_everything_command(command).expect("Can extract command");
            let Some(snapshot::EncodingCommand::Encoding {
                encoding: snapshot::Encoding::Parquet { compression, .. },
            }) = encoding_command
            else {
                panic!("Expected a snapshot everything command with parquet encoding");
            };
            assert_eq!(compression, 7);
        }

        #[cfg(feature = "parquet")]
        #[test]
        fn compression_has_a_default() {
            // given
            let line =
                "./core snapshot --output-directory dir everything encoding parquet";

            // when
            let command = parse_cli(line, "")
                .expect("should parse the snapshot command")
                .command;

            // then
            let (_, _, encoding_command) =
                extract_everything_command(command).expect("Can extract command");
            let Some(snapshot::EncodingCommand::Encoding {
                encoding: snapshot::Encoding::Parquet { compression, .. },
            }) = encoding_command
            else {
                panic!("Expected a snapshot everything command with parquet encoding");
            };

            assert_eq!(compression, 1);
        }

        #[test]
        fn json_encoding_doesnt_allow_for_group_size() {
            // given
            let line =
                "./core snapshot --output-directory dir everything encoding json --group-size 101";

            // when
            let result = parse_cli(line, "");

            // then
            assert!(result.is_err());
        }
    }

    mod snapshot_contract_tests {
        use super::*;

        #[test]
        fn snapshot_contract() {
            // given
            let line = "./core snapshot --output-directory ./snapshot contract";
            let irrelevant_remainder =
                "--id 0x0000000000000000000000000000000000000000000000000000000000000000";

            // when
            let command = parse_cli(line, irrelevant_remainder)
                .expect("should parse the snapshot command")
                .command;

            // then
            let Fuel::Snapshot(snapshot::Command {
                subcommand: snapshot::SubCommands::Contract { .. },
                ..
            }) = command
            else {
                panic!("Expected a snapshot contract command");
            };
        }

        #[test]
        fn snapshot_contract_id_required() {
            // given
            let line = "./core snapshot --output-directory ./snapshot contract";

            // when
            let result = parse_cli(line, "");

            // then
            assert!(result.is_err());
        }

        #[test]
        fn snapshot_contract_id_given() {
            // given
            let line = "./core snapshot --output-directory ./snapshot contract --id 0x1111111111111111111111111111111111111111111111111111111111111111";

            // when
            let command = parse_cli(line, "")
                .expect("should parse the snapshot command")
                .command;

            // then
            let Fuel::Snapshot(snapshot::Command {
                subcommand: snapshot::SubCommands::Contract { contract_id },
                ..
            }) = command
            else {
                panic!("Expected a snapshot contract command");
            };
            assert_eq!(contract_id, ContractId::from([0x11u8; 32]));
        }
    }

    mod run_arg_tests {
        use std::path::PathBuf;

        use crate::cli::run;

        #[test]
        fn can_ask_for_db_prune() {
            // given
            let line = "./core run --db-prune";

            // when
            let command = super::parse_cli(line, "")
                .expect("should parse the run command")
                .command;

            // then
            let super::Fuel::Run(run::Command { db_prune, .. }) = command else {
                panic!("Expected a run command");
            };

            assert!(db_prune);
        }

        #[test]
        fn db_prune_off_by_default() {
            // given
            let line = "./core run";

            // when
            let command = super::parse_cli(line, "")
                .expect("should parse the run command")
                .command;

            // then
            let super::Fuel::Run(run::Command { db_prune, .. }) = command else {
                panic!("Expected a run command");
            };

            assert!(!db_prune);
        }

        #[test]
        fn can_give_a_snapshot() {
            // given
            let line = "./core run --snapshot ./some/path";

            // when
            let command = super::parse_cli(line, "")
                .expect("should parse the run command")
                .command;

            // then
            let super::Fuel::Run(run::Command {
                snapshot: Some(snapshot),
                ..
            }) = command
            else {
                panic!("Expected a run command");
            };

            assert_eq!(snapshot, PathBuf::from("./some/path"));
        }

        #[test]
        fn snapshot_is_optional() {
            // given
            let line = "./core run";

            // when
            let command = super::parse_cli(line, "")
                .expect("should parse the run command")
                .command;

            // then
            let super::Fuel::Run(run::Command { snapshot: None, .. }) = command else {
                panic!("Expected a run command without a snapshot");
            };
        }
    }
}
