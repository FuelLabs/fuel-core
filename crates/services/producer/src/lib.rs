#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(unused_must_use)]
#![deny(warnings)]

pub mod block_producer;
pub mod config;
pub mod ports;

pub use block_producer::Producer;
pub use config::Config;

#[cfg(any(test, feature = "test-helpers"))]
pub mod mocks;

#[cfg(test)]
fuel_core_trace::enable_tracing!();
