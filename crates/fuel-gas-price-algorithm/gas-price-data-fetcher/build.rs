#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

use std::fs;

fn main() {
    fs::create_dir_all("target").expect("Unable to create target directory");
    fs::write("target/schema.sdl", fuel_core_client::SCHEMA_SDL)
        .expect("Unable to write schema file");

    println!("cargo:rerun-if-changed=build.rs");
}
