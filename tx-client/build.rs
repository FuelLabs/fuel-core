use fuel_core::schema::tx;

use std::env;
use std::fs::{self, File};
use std::io::prelude::*;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=../fuel-core/src/schema/tx.rs");

    let assets = env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .map(|f| {
            let f = f.as_path().join("assets/tx.sdl");

            let dir = f.parent().expect("Failed to read assets dir");
            fs::create_dir_all(dir).expect("Failed to create assets dir");

            f
        })
        .expect("Failed to fetch assets path");

    File::create(&assets)
        .and_then(|mut f| {
            f.write_all(tx::schema(None).sdl().as_bytes())?;
            f.sync_all()
        })
        .expect("Failed to write SDL schema to temporary file");
}
