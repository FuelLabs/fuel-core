#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

#[cfg(feature = "wasm-executor")]
use wasmtime::{
    Config,
    Engine,
    Module,
};

pub mod config;
pub mod executor;

#[cfg(feature = "wasm-executor")]
pub mod instance;

/// The WASM version of the underlying [`fuel_core_executor::executor::ExecutionInstance`].
#[cfg(feature = "wasm-executor")]
pub const WASM_BYTECODE: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/wasm32-unknown-unknown/release/fuel-core-wasm-executor.wasm"
));

#[cfg(feature = "wasm-executor")]
lazy_static::lazy_static! {
    /// The default engine for the WASM executor. It is used to compile the WASM bytecode.
    pub static ref DEFAULT_ENGINE: Engine = {
        Engine::new(&Config::new()).expect("Failed to instantiate the `Engine`")
    };

    /// The default module compiles the WASM bytecode of the native executor.
    /// It is used to create the WASM instance of the executor.
    pub static ref COMPILED_UNDERLYING_EXECUTOR: Module = {
        Module::new(&DEFAULT_ENGINE, WASM_BYTECODE).expect("Failed to compile the underlying executor")
    };
}
