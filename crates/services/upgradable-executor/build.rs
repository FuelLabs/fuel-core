use std::{
    env,
    path::{
        Path,
        PathBuf,
    },
    process::Command,
};

fn main() {
    let wasm_executor_enabled = env::var_os("CARGO_FEATURE_WASM_EXECUTOR").is_some();

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wasm-executor/src/*");

    if !wasm_executor_enabled {
        return;
    }

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir);

    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    let target_dir = format!("--target-dir={}", dest_path.to_string_lossy());

    let args = vec![
        "build".to_owned(),
        "-p".to_owned(),
        "fuel-core-wasm-executor".to_owned(),
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
    cargo.current_dir(project_root()).args(args);

    let output = cargo.output();

    match output {
        Ok(output) => {
            if !output.status.success() {
                println!(
                    "cargo:warning=Got an error status during compiling WASM executor: {:?}",
                    output
                );
            }
        }
        Err(err) => {
            println!("cargo:warning={:?}", err);
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
