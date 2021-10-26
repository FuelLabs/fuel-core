use fuel_core::wasm_executor::{IndexExecutor, Manifest, SchemaManager};
use fuel_indexer::types::*;
use serde_json;
use std::fs;
use std::io::Read;
use structopt::StructOpt;

#[derive(StructOpt)]
struct Options {
    #[structopt(short, long)]
    wasm: String,
    #[structopt(short, long)]
    manifest: String,
}

fn main() {
    let database = dotenv::var("DATABASE_URL").expect("No database provided");

    let args = Options::from_args();
    let mut f = fs::File::open(&args.wasm).expect("Could not open wasm file");

    let mut wasm_bytes = Vec::new();
    f.read_to_end(&mut wasm_bytes).expect("Failed to read wasm");

    let mut f = fs::File::open(&args.manifest).expect("Could not open manifest file");

    let mut yaml = String::new();
    f.read_to_string(&mut yaml)
        .expect("Failed to read manifest file.");
    let manifest: Manifest = serde_yaml::from_str(&yaml).expect("Bad manifest file.");

    let schema_manager = SchemaManager::new(database.clone()).expect("Schema manager failed");

    let mut schema = String::new();
    let mut f = fs::File::open(&manifest.graphql_schema).expect("Could not open graphql schema");
    f.read_to_string(&mut schema)
        .expect("Could not read graphql schema");

    schema_manager
        .new_schema(&manifest.namespace, &schema)
        .expect("Could not create new schema");

    let test_events = manifest.test_events.clone();

    let instance =
        IndexExecutor::new(database, manifest, wasm_bytes).expect("Error creating IndexExecutor");

    for event in test_events {
        if event.trigger == "an_event_name" {
            let evt: SomeEvent = serde_json::from_str(&event.payload).expect("Bad payload value");
            instance
                .trigger_event("an_event_name", serialize(&evt))
                .expect("Indexing failed");
        } else if event.trigger == "another_event_name" {
            let evt: AnotherEvent =
                serde_json::from_str(&event.payload).expect("Bad payload value");
            instance
                .trigger_event("another_event_name", serialize(&evt))
                .expect("Indexing failed");
        } else {
            println!("NO handler for {}", event.trigger);
        }
        println!("done!");
    }
}
