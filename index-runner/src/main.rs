use clap::{Arg, App};
use std::fs;
use std::io::Read;
use fuel_core::runtime::{IndexExecutor, Manifest, SchemaManager};

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
        .arg(Arg::with_name("manifest")
            .short("m")
            .long("manifest")
            .value_name("MANIFEST_FILE")
            .help("Specify a manifest yaml file.")
            .required(true)
            .takes_value(true)
        )
        .get_matches();

    let filename = matches.value_of("wasm").unwrap();
    let mut f = fs::File::open(filename).expect("Could not open wasm file");

    let mut wasm_bytes = Vec::new();
    f.read_to_end(&mut wasm_bytes).expect("Failed to read wasm");

    let manifest_file = matches.value_of("manifest").unwrap();
    let mut f = fs::File::open(manifest_file).expect("Could not open manifest file");

    let mut yaml = String::new();
    f.read_to_string(&mut yaml).expect("Failed to read manifest file.");
    let manifest: Manifest = serde_yaml::from_str(&yaml).expect("Bad manifest file.");

    let schema_manager = SchemaManager::new(
        "postgres://postgres:my-secret@127.0.0.1:5432".to_string(),
    ).expect("Schema manager failed");

    let mut sql = String::new();
    let mut f = fs::File::open(&manifest.postgres_schema).expect("Failed reading manifest");
    f.read_to_string(&mut sql).expect("Failed reading manifest");
    schema_manager.new_schema(&manifest.namespace, &sql).expect("Could not create new schema");

    let instance = IndexExecutor::new(
        "postgres://postgres:my-secret@127.0.0.1:5432".to_string(),
        manifest,
        wasm_bytes,
    ).expect("Error creating IndexExecutor");
    instance
        .trigger_event("an_event_name")
        .expect("Indexing failed");
    instance
        .trigger_event("another_event_name")
        .expect("Indexing failed");
}
