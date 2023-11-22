#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

mod config;

pub mod refs;
pub mod ports;
pub mod executor;

pub struct BlockExecutor {}

pub use config::Config;

#[cfg(test)]
fuel_core_trace::enable_tracing!();
