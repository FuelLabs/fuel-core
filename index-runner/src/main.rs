use clap::{Arg, App};
use std::fs;
use std::io::Read;
use fuel_core::indexer::IndexExecutor;

fn main() {
    let matches = App::new("Standalone index runner")
        .version("0.1")
        .about("Runs a wasm index standalone from server")
        .arg(Arg::with_name("wasm")
            .short("w")
            .long("wasm")
            .value_name("WASM_FILE")
            .help("Specify a wasm file (wat or wasm)")
            .required(true)
            .takes_value(true)
        )
        .get_matches();

    let filename = matches.value_of("wasm").unwrap();
    let mut f = fs::File::open(filename).expect("Could not open wasm file");

    let mut wasm_bytes = Vec::new();
    f.read_to_end(&mut wasm_bytes).expect("Failed to read wasm");

    let instance = IndexExecutor::new(&wasm_bytes).expect("Error creating IndexExecutor");
    instance.run_indexer().expect("Indexing failed");

    // TODO: maybe save/restore, and query options
}
