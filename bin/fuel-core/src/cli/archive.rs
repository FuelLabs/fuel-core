#[cfg(feature = "archive")]
mod archive;
mod backup;
mod restore;

use crate::cli::archive::{
    backup::backup,
    restore::restore,
};
use clap::{
    Parser,
    Subcommand,
};

#[derive(Debug, Subcommand)]
pub enum Command {
    Backup(BackupArgs),
    Restore(RestoreArgs),
}

#[derive(Debug, Parser)]
pub struct BackupArgs {
    #[arg(long)]
    pub from: String,

    #[arg(long)]
    pub to: String,
}

#[derive(Debug, Parser)]
pub struct RestoreArgs {
    #[arg(long)]
    pub from: String,

    #[arg(long)]
    pub to: String,
}

pub async fn exec(command: Command) -> anyhow::Result<()> {
    match command {
        Command::Backup(args) => backup(&args.from, &args.to),
        Command::Restore(args) => restore(&args.from, &args.to),
    }
}
