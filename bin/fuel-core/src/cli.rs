use clap::Parser;
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

lazy_static::lazy_static! {
    pub static ref DEFAULT_DB_PATH: PathBuf = dirs::home_dir().unwrap().join(".fuel").join("db");
}

pub mod fee_contract;
pub mod run;
pub mod snapshot;

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
    Snapshot(snapshot::Command),
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
            return run::exec(command).await
        }
    }

    match opt {
        Ok(opt) => match opt.command {
            Fuel::Run(command) => run::exec(command).await,
            Fuel::Snapshot(command) => snapshot::exec(command),
            Fuel::GenerateFeeContract(command) => fee_contract::exec(command).await,
        },
        Err(e) => {
            // Prints the error and exits.
            e.exit()
        }
    }
}

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
        use crate::cli::DEFAULT_DB_PATH;

        use super::*;

        #[test]
        fn can_snapshot() {
            // given
            let line = "./core snapshot";
            let irrelevant_remainder = "everything --output-directory dir encoding json";

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
            let irrelevant_remainder = "everything --output-directory dir encoding json";

            // when
            let command = parse_cli(line, irrelevant_remainder)
                .expect("should parse the snapshot command")
                .command;

            // then
            let Fuel::Snapshot(snapshot::Command { database_path, .. }) = command else {
                panic!("Expected a snapshot command")
            };
            assert_eq!(database_path, DEFAULT_DB_PATH.as_path());
        }

        #[test]
        fn db_is_as_given() {
            // given
            let line = "./core snapshot --db-path ./some/path";
            let irrelevant_remainder = "everything --output-directory dir encoding json";

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
                            output_dir,
                            encoding_command,
                        },
                    ..
                }) => Ok((chain_config, output_dir, encoding_command)),
                _ => bail!("Expected a snapshot everything command"),
            }
        }

        #[test]
        fn snapshot_everything() {
            // given
            let line = "./core snapshot everything";
            let irrelevant_remainder = "--output-directory dir encoding json";

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
        fn output_dir_is_as_given() {
            // given
            let line = "./core snapshot everything --output-directory ./some/path";

            // when
            let command = parse_cli(line, "")
                .expect("should parse the snapshot command")
                .command;

            // then
            let (_, output_dir, _) =
                extract_everything_command(command).expect("Can extract command");
            assert_eq!(output_dir, PathBuf::from("./some/path"));
        }

        #[test]
        fn encoding_is_optional() {
            // given
            let line = "./core snapshot everything --output-directory ./some/path";
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
        fn can_choose_json_encoding() {
            // given
            let line = "./core snapshot everything --output-directory dir encoding json";

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

        #[test]
        fn can_choose_parquet_encoding() {
            // given
            let line =
                "./core snapshot everything --output-directory dir encoding parquet";

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

        #[test]
        fn group_size_is_configurable() {
            // given
            let line =
                "./core snapshot everything --output-directory dir encoding parquet --group-size 101";

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

        #[test]
        fn group_size_has_a_default() {
            // given
            let line =
                "./core snapshot everything --output-directory dir encoding parquet";

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

        #[test]
        fn can_configure_compression() {
            // given
            let line =
                "./core snapshot everything --output-directory dir encoding parquet --compression-level 7";

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

        #[test]
        fn compression_has_a_default() {
            // given
            let line =
                "./core snapshot everything --output-directory dir encoding parquet";

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
                "./core snapshot everything --output-directory dir encoding json --group-size 101";

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
            let line = "./core snapshot contract";
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
            let line = "./core snapshot contract";

            // when
            let result = parse_cli(line, "");

            // then
            assert!(result.is_err());
        }

        #[test]
        fn snapshot_contract_id_given() {
            // given
            let line = "./core snapshot contract --id 0x1111111111111111111111111111111111111111111111111111111111111111";

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
}
