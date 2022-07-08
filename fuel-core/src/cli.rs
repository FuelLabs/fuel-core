use anyhow::Result;
use clap::Parser;
use std::{env, path::PathBuf, str::FromStr};
use tracing::log::warn;
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
    command: Fuel,
}

#[derive(Debug, Parser)]
pub enum Fuel {
    Run(run::Command),
    Snapshot(snapshot::Command),
}

pub const LOG_FILTER: &str = "RUST_LOG";
pub const HUMAN_LOGGING: &str = "HUMAN_LOGGING";

pub async fn init_logging() -> Result<()> {
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
    Ok(())
}

pub async fn run_cli() -> Result<()> {
    init_logging().await?;

    let opt = Opt::try_parse();
    if opt.is_err() {
        let command = run::Command::try_parse();
        if let Ok(command) = command {
            warn!("This cli format for running `fuel-core` is deprecated and will be removed. Please use `fuel-core run` or use `--help` for more information");
            return run::exec(command).await;
        }
    }

    let opt = opt?;

    match opt.command {
        Fuel::Run(command) => run::exec(command).await,
        Fuel::Snapshot(command) => snapshot::exec(command).await,
    }
}
