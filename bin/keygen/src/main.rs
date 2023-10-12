//! A simple keygen cli utility tool for configuring fuel-core

use clap::Parser;
use fuel_core_keygen::keygen;

/// Key management utilities for configuring fuel-core
#[derive(Debug, Parser)]
pub(crate) enum Command {
    New(keygen::NewKey),
    Parse(keygen::ParseSecret),
}

impl Command {
    pub(crate) fn exec(&self) -> anyhow::Result<()> {
        match self {
            Command::New(cmd) => cmd.exec(),
            Command::Parse(cmd) => cmd.exec(),
        }
    }
}

fn main() -> anyhow::Result<()> {
    let cmd = Command::parse();
    cmd.exec()
}
