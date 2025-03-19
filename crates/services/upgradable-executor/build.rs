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
    // Check if wasm-tools is available
    if !Command::new("wasm-tools")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
    {
        println!("cargo:warning=wasm-tools not found! This is required for building the WASM executor.");
        println!("cargo:warning=Please install wasm-tools using one of these methods:");
        println!("cargo:warning=  - cargo install wasm-tools");
        println!("cargo:warning=  - brew install wasm-tools       (on macOS)");
        println!("cargo:warning=  - your system package manager");
        panic!("wasm-tools is required but not found");
    }

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

    #[cfg(feature = "fault-proving")]
    args.extend(["--features".to_owned(), "fault-proving".to_owned()]);

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
    cargo.env("RUSTFLAGS", "-Ctarget-cpu=mvp");

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

    // Canonicalize the WASM binary
    let wasm_path = dest_path.join("bin").join("fuel-core-wasm-executor.wasm");
    if let Err(e) = canonicalize_wasm(&wasm_path) {
        panic!("Failed to canonicalize WASM binary: {}", e);
    }
}

#[cfg(feature = "wasm-executor")]
// The WASM binary produced by the 1.85.0 compiler may contain non-canonical LEB128 encodings
// (e.g. encoding 0 as 80 80 80 80 00 instead of just 00). While these are valid per the
// spec, some runtimes (like wasmtime) require canonical encodings in MVP mode.
//
// To fix this, we convert the WASM to text format (WAT) and back, which ensures
// canonical LEB128 encodings in the final binary.
fn canonicalize_wasm(wasm_path: &Path) -> Result<(), String> {
    let temp_wat = wasm_path.with_extension("temp.wat");
    let temp_wasm = wasm_path.with_extension("temp.wasm");

    // Step 1: Convert to WAT
    let status = Command::new("wasm-tools")
        .arg("print")
        .arg(wasm_path)
        .arg("-o")
        .arg(&temp_wat)
        .status()
        .map_err(|e| format!("Failed to run wasm-tools print: {}", e))?;

    if !status.success() {
        return Err("wasm-tools print failed".to_string());
    }

    // Step 2: Convert back to WASM (this produces canonical LEB128s)
    let status = Command::new("wasm-tools")
        .arg("parse")
        .arg(&temp_wat)
        .arg("-o")
        .arg(&temp_wasm)
        .status()
        .map_err(|e| format!("Failed to run wasm-tools parse: {}", e))?;

    if !status.success() {
        return Err("wasm-tools parse failed".to_string());
    }

    // Step 3: Replace original with canonical version
    std::fs::rename(&temp_wasm, wasm_path)
        .map_err(|e| format!("Failed to replace WASM with canonical version: {}", e))?;

    // Clean up temporary WAT file
    if let Err(e) = std::fs::remove_file(&temp_wat) {
        eprintln!("Warning: Failed to clean up temporary WAT file: {}", e);
    }

    Ok(())
}

#[cfg(feature = "wasm-executor")]
fn project_root() -> PathBuf {
    Path::new(&env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .unwrap()
        .to_path_buf()
}
