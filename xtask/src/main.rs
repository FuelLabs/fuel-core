use commands::build::{cargo_build_and_dump_schema, BuildCommand};
use structopt::StructOpt;

mod commands;

#[derive(Debug, StructOpt)]
#[structopt(name = "xtask", about = "fuel-core dev builder")]
pub struct Opt {
    #[structopt(subcommand)]
    command: Xtask,
}

#[derive(Debug, StructOpt)]
enum Xtask {
    Build(BuildCommand),
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();

    match opt.command {
        Xtask::Build(_) => cargo_build_and_dump_schema(),
    }
}
