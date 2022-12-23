//! A simple keygen cli utility tool for configuring fuel-core

use clap::Parser;

pub mod keygen;

fn main() -> anyhow::Result<()> {
    let cmd = keygen::Command::parse();
    cmd.exec()
}
