#![deny(unused_crate_dependencies)]
#![deny(warnings)]

use clap::Parser;
use commands::{
    build::{
        cargo_build_and_dump_schema,
        BuildCommand,
    },
    dump::{
        dump_schema,
        DumpCommand,
    },
};

mod commands;

#[derive(Debug, Parser)]
#[clap(name = "xtask", about = "fuel-core dev builder", version)]
pub struct Opt {
    #[clap(subcommand)]
    command: Xtask,
}

#[derive(Debug, Parser)]
enum Xtask {
    Build(BuildCommand),
    Dump(DumpCommand),
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::parse();

    match opt.command {
        Xtask::Build(_) => cargo_build_and_dump_schema(),
        Xtask::Dump(_) => dump_schema(),
    }
}
