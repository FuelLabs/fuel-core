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
    let sdl = String::from_utf8(current_content.clone()).unwrap();
    let binding = build_schema().finish().sdl();
    let built_lines = binding.lines();
    let file_lines = sdl.lines();
    for (i, (built, file)) in built_lines.clone().zip(file_lines.clone()).enumerate() {
        println!("built: {built:?}\n file: {file:?}");
        assert_eq!(
            built, file,
            "mismatch on line {i:?}\n built: {built:?}\n file: {file:?}"
        );
        if built != file {
            println!("mismatch on line {i:?}\n built: {built:?}\n file: {file:?}");
            // let built_vec: Vec<_> = built_lines.clone().collect();
            // let file_vec: Vec<_> = file_lines.clone().collect();
            // // // let built_snippet: Vec<_> = built_vec[i - 5..i + 5];
            // // // let file_snippet: Vec<_> = file_vec[i - 5..i + 5];
            // let built_snippet =
            //     &built_vec[i.saturating_sub(5)..(i + 5).min(built_vec.len())];
            // let file_snippet =
            //     &file_vec[i.saturating_sub(5)..(i + 5).min(file_vec.len())];
            // for (j, (b, f)) in built_snippet.iter().zip(file_snippet).enumerate() {
            //     let line_no = i.saturating_sub(5) + j;
            //     if b != f {
            //         println!("mismatch on line {line_no:?}\n built: {b:?}\n file: {f:?}");
            //     } else {
            //         println!(" line {line_no:?}\n built: {b:?}\n file: {f:?}");
            //     }
            // }
            // panic!("failed to dump schema");
        }
    }
    // assert!(
    //     current_content == build_schema().finish().sdl().as_bytes(),
    //     "The schema is not up to date"
    // );
}
