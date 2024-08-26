#[cfg(feature = "wasm-executor")]
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
    #[cfg(feature = "wasm-executor")]
    build_wasm()
}

#[cfg(feature = "wasm-executor")]
fn build_wasm() {
    let out_dir = env::var_os("OUT_DIR").expect("The output directory is not set");
    let dest_path = Path::new(&out_dir);
    let bin_dir = format!("--root={}", dest_path.to_string_lossy());

    // Set the own sub-target directory to prevent a cargo deadlock (cargo locks
    // a target dir exclusive). This directory is also used as a cache directory
    // to avoid building the same WASM binary each time. Also, caching reuses
    // the artifacts from the previous build in the case if the executor is changed.
    //
    // The cache dir is: "target/{debug/release/other}/fuel-core-upgradable-executor-cache"
    let mut cache_dir: std::path::PathBuf = out_dir.clone().into();
    cache_dir.pop();
    cache_dir.pop();
    cache_dir.pop();
    cache_dir.push("fuel-core-upgradable-executor-cache");
    let target_dir = format!("--target-dir={}", cache_dir.to_string_lossy());

    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    let mut args = vec![
        "install".to_owned(),
        "--target=wasm32-unknown-unknown".to_owned(),
        "--no-default-features".to_owned(),
        "--locked".to_owned(),
        target_dir,
        bin_dir,
    ];

    #[cfg(feature = "smt")]
    args.extend(["--features".to_owned(), "smt".to_owned()]);

    let manifest_dir =
        env::var_os("CARGO_MANIFEST_DIR").expect("The manifest directory is not set");
    let manifest_path = Path::new(&manifest_dir);
    let wasm_executor_path = manifest_path.join("wasm-executor");

    // If the `wasm-executor` source code is available, then use it as a source for the
    // `wasm-executor` binary. Otherwise, use the version from the crates.io.
    if wasm_executor_path.exists() {
        args.extend([
            "--path".to_owned(),
            wasm_executor_path.to_string_lossy().to_string(),
        ]);
    } else {
        let crate_version = env!("CARGO_PKG_VERSION");
        args.extend([
            "--version".to_owned(),
            crate_version.to_owned(),
            "fuel-core-wasm-executor".to_string(),
        ]);
    }

    let mut cargo = Command::new(cargo);
    cargo.env("CARGO_PROFILE_RELEASE_LTO", "true");
    cargo.env("CARGO_PROFILE_RELEASE_PANIC", "abort");
    cargo.env("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "1");
    cargo.env("CARGO_PROFILE_RELEASE_OPT_LEVEL", "3");
    cargo.env("CARGO_PROFILE_RELEASE_STRIP", "symbols");
    cargo.env("CARGO_PROFILE_RELEASE_DEBUG", "false");
    if env::var("CARGO_CFG_COVERAGE").is_ok() {
        // wasm doesn't support coverage
        cargo.env("CARGO_ENCODED_RUSTFLAGS", "");
    }
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

#[cfg(feature = "wasm-executor")]
fn project_root() -> PathBuf {
    Path::new(&env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .unwrap()
        .to_path_buf()
}
