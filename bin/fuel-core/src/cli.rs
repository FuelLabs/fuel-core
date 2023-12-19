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

pub async fn init_logging() -> anyhow::Result<()> {
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
    Ok(())
}

pub async fn run_cli() -> anyhow::Result<()> {
    init_logging().await?;
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
            Fuel::Snapshot(command) => snapshot::exec(command).await,
            Fuel::GenerateFeeContract(command) => fee_contract::exec(command).await,
        },
        Err(e) => {
            // Prints the error and exits.
            e.exit()
        }
    }
}
