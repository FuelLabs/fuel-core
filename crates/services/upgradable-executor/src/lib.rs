#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

pub mod config;
pub mod error;
pub mod executor;

#[cfg(feature = "wasm-executor")]
pub mod instance;

/// The WASM version of the underlying [`fuel_core_executor::executor::ExecutionInstance`].
#[cfg(feature = "wasm-executor")]
pub const WASM_BYTECODE: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/bin/fuel-core-wasm-executor.wasm"
));
