use clap::Parser;
use fuel_core::schema::build_schema;
use std::{
    env,
    fs::{
        self,
        File,
    },
    io::Write,
    path::PathBuf,
};

// stored in the root of the workspace
const SCHEMA_URL: &str = "../crates/client/assets/schema.sdl";

#[derive(Debug, Parser)]
pub struct DumpCommand {}

pub fn dump_schema() -> Result<(), Box<dyn std::error::Error>> {
    let assets = env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .map(|f| {
            let f = f.as_path().join(SCHEMA_URL);
            let dir = f.parent().expect("Failed to read assets dir");
            fs::create_dir_all(dir).expect("Failed to create assets dir");

            f
        })
        .expect("Failed to fetch assets path");

    File::create(assets)
        .and_then(|mut f| {
            f.write_all(build_schema().finish().sdl().as_bytes())?;
            f.sync_all()
        })
        .expect("Failed to write SDL schema to temporary file");

    Ok(())
}

/// ensures that latest schema is always committed
#[test]
fn is_latest_schema_committed() {
    let current_content = fs::read(SCHEMA_URL).unwrap();
    assert_eq!(current_content, build_schema().finish().sdl().as_bytes());
}
