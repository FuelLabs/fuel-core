use std::{
    env,
    path::{
        Path,
        PathBuf,
    },
    process::Command,
};

fn main() {
    // It only forces a rerun of the build when `build.rs` is changed.
    println!("cargo:rerun-if-changed=build.rs");
    // Because `fuel-core-wasm-executor 0.26.0` is using `--offline` flag,
    // we need to download all dependencies before building the WASM executor.
    // This is a workaround specifically for `fuel-core 0.26.0`.
    // Future version of the `fuel-core` will not use `--offline` flag.
    build_fuel_core_26()
}

fn build_fuel_core_26() {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let args = vec![
        "install".to_owned(),
        "fuel-core-wasm-executor".to_owned(),
        "--target=wasm32-unknown-unknown".to_owned(),
        "--no-default-features".to_owned(),
        "--locked".to_owned(),
        "--version".to_owned(),
        "0.26.0".to_owned(),
    ];

    let mut cargo = Command::new(cargo);
    cargo.current_dir(project_root()).args(args);

    let output = cargo.output();

    match output {
        Ok(output) => {
            if !output.status.success() {
                panic!(
                    "Got an error status during compiling WASM executor: \n{:#?}",
                    output
                );
            }
        }
        Err(err) => {
            panic!("\n{:#?}", err);
        }
    }
}

fn project_root() -> PathBuf {
    Path::new(&env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .unwrap()
        .to_path_buf()
}
