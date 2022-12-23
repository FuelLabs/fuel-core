use clap::Parser;

pub mod keygen;

fn main() -> anyhow::Result<()> {
    let cmd = keygen::Command::parse();
    cmd.exec()
}
