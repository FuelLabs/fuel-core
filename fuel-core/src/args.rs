use crate::args::{run::RunCommand, snapshot::SnapshotCommand};
use anyhow::Result;
use clap::Parser;
use std::{env, path::PathBuf, str::FromStr};
use tracing_subscriber::filter::EnvFilter;

lazy_static::lazy_static! {
    pub static ref DEFAULT_DB_PATH: PathBuf = dirs::home_dir().unwrap().join(".fuel").join("db");
}

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
    command: Core,
}

#[derive(Debug, Parser)]
pub enum Core {
    Snapshot(SnapshotCommand),
    Run(RunCommand),
}

pub const LOG_FILTER: &str = "RUST_LOG";
pub const HUMAN_LOGGING: &str = "HUMAN_LOGGING";

pub async fn run_cli() -> Result<()> {
    let opt = Opt::parse();

    let filter = match env::var_os(LOG_FILTER) {
        Some(_) => EnvFilter::try_from_default_env().expect("Invalid `RUST_LOG` provided"),
        None => EnvFilter::new("info"),
    };

    let human_logging = env::var_os(HUMAN_LOGGING)
        .map(|s| {
            bool::from_str(s.to_str().unwrap())
                .expect("Expected `true` or `false` to be provided for `HUMAN_LOGGING`")
        })
        .unwrap_or(true);

    let sub = tracing_subscriber::fmt::Subscriber::builder()
        .with_writer(std::io::stderr)
        .with_env_filter(filter);

    if human_logging {
        // use pretty logs
        sub.with_ansi(true)
            .with_level(true)
            .with_line_number(true)
            .init();
    } else {
        // use machine parseable structured logs
        sub
            // disable terminal colors
            .with_ansi(false)
            .with_level(true)
            .with_line_number(true)
            // use json
            .json()
            .init();
    }
    match opt.command {
        Core::Snapshot(command) => snapshot::exec(command).await,
        Core::Run(command) => run::exec(command).await,
    }
}
