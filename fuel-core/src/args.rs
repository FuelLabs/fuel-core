use crate::args::run::RunCommand;
use crate::args::snapshot::SnapshotCommand;
use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

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

pub async fn run_cli() -> Result<()> {
    let opt = Opt::parse();
    match opt.command {
        Core::Snapshot(command) => snapshot::exec(command).await,
        Core::Run(command) => run::exec(command).await,
    }
}
