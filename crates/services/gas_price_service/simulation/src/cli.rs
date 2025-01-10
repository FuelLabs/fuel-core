use clap::{
    Parser,
    Subcommand,
};
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum SimulationArgs {
    Single {
        /// DA P component
        #[arg(short, long)]
        p_component: i64,
        /// DA D component
        #[arg(short, long)]
        d_component: i64,
        /// Latest gas price
        #[arg(short, long)]
        start_gas_price: u64,
    },
}

#[derive(Parser, Debug)]
pub struct Args {
    /// Which simulation mode to run
    #[command(subcommand)]
    pub simulation: SimulationArgs,
    /// Data source file
    #[arg(short, long)]
    pub file_path: PathBuf,
}
