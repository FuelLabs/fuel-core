#![deny(unused_crate_dependencies)]
#![deny(warnings)]

mod config;

pub mod refs;

pub struct BlockExecutor {}

pub use config::Config;

#[cfg(test)]
fuel_core_trace::enable_tracing!();
