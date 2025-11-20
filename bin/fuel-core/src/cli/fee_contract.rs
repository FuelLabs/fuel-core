use clap::Parser;
use fuel_core_chain_config::fee_collection_contract;
use fuel_core_types::fuel_tx::Address;
use std::{fs::OpenOptions, io::Write, path::PathBuf};

#[derive(Debug, Parser)]
pub struct Command {
    /// Address to withdraw fees to
    withdrawal_address: Address,
    /// Output file. If not provided, will print hex representation to stdout.
    #[clap(short, long)]
    output: Option<PathBuf>,
    /// Overwrite output file if it exists. No effect if `output` is not provided.
    #[clap(short, long)]
    force: bool,
}

pub async fn exec(cmd: Command) -> anyhow::Result<()> {
    let contract = fee_collection_contract::generate(cmd.withdrawal_address);

    if let Some(output) = cmd.output.as_ref() {
        let mut open_opt = OpenOptions::new();
        if cmd.force {
            open_opt.create(true).write(true).truncate(true);
        } else {
            open_opt.create_new(true).write(true);
        }

        let mut file = open_opt.open(output)?;
        file.write_all(&contract)?;
    } else {
        println!("{}", hex::encode(contract));
    }

    Ok(())
}
