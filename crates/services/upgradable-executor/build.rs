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
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wasm-executor/src/main.rs");
    println!("cargo:rerun-if-changed=src/executor.rs");

    #[cfg(feature = "wasm-executor")]
    build_wasm()
}

#[cfg(feature = "wasm-executor")]
fn build_wasm() {
    let out_dir = env::var_os("OUT_DIR").expect("The output directory is not set");
    let dest_path = Path::new(&out_dir);
    let manifest_dir =
        env::var_os("CARGO_MANIFEST_DIR").expect("The manifest directory is not set");
    let manifest_path = Path::new(&manifest_dir);
    let wasm_executor_path = manifest_path
        .join("wasm-executor")
        .join("Cargo.toml")
        .to_string_lossy()
        .to_string();

    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    let target_dir = format!("--target-dir={}", dest_path.to_string_lossy());

    let args = vec![
        "build".to_owned(),
        "--manifest-path".to_owned(),
        wasm_executor_path,
        "--target=wasm32-unknown-unknown".to_owned(),
        "--no-default-features".to_owned(),
        "--release".to_owned(),
        target_dir,
    ];

    let mut cargo = Command::new(cargo);
    cargo.env("CARGO_PROFILE_RELEASE_LTO", "true");
    cargo.env("CARGO_PROFILE_RELEASE_PANIC", "abort");
    cargo.env("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "1");
    cargo.env("CARGO_PROFILE_RELEASE_OPT_LEVEL", "3");
    cargo.env("CARGO_PROFILE_RELEASE_STRIP", "symbols");
    cargo.env("CARGO_PROFILE_RELEASE_DEBUG", "false");
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
