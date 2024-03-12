use wasmtime::{
    Config,
    Engine,
    Module,
};

pub mod config;
pub mod executor;
pub mod instance;

pub const WASM_BYTECODE: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/wasm32-unknown-unknown/release/fuel-core-wasm-executor.wasm"
));

lazy_static::lazy_static! {
    pub static ref DEFAULT_ENGINE: Engine = {
        Engine::new(&Config::new()).expect("Failed to instantiate the `Engine`")
    };

    pub static ref COMPILED_UNDERLYING_EXECUTOR: Module = {
        Module::new(&DEFAULT_ENGINE, WASM_BYTECODE).expect("Failed to compile the underlying executor")
    };
}
