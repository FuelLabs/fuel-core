pub mod asc_abi;

mod host;
pub mod to_from;

/// Public interface of the crate, receives triggers to be processed.

/// Pre-processes modules and manages their threads. Serves as an interface from `host` to `module`.
pub mod mapping;

/// WASM module instance.
pub mod module;

/// Runtime-agnostic implementation of exports to WASM.
pub mod host_exports;

pub mod error;
mod gas_rules;

pub use host::RuntimeHostBuilder;
pub use host_exports::HostExports;
pub use mapping::{MappingContext, ValidModule};
pub use module::{ExperimentalFeatures, WasmInstance};
