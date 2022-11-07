use super::dump::dump_schema;
use clap::Parser;
use std::{
    env,
    path::{
        Path,
        PathBuf,
    },
    process::Command,
};

#[derive(Debug, Parser)]
pub struct BuildCommand {}

pub fn cargo_build_and_dump_schema() -> Result<(), Box<dyn std::error::Error>> {
    dump_schema()?;

    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .current_dir(project_root())
        .args(["build"])
        .status()?;

    if !status.success() {
        return Err("cargo build failed".into())
    }

    Ok(())
}

fn project_root() -> PathBuf {
    Path::new(&env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .unwrap()
        .to_path_buf()
}
