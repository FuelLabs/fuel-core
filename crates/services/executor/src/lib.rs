#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

mod config;

pub mod executor;
pub mod ports;
pub mod refs;

pub struct BlockExecutor {}

pub use config::Config;

#[cfg(test)]
fuel_core_trace::enable_tracing!();
