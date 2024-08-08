#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

pub mod cached_storage;
pub mod executor;
pub mod ports;
pub mod refs;

#[cfg(test)]
fuel_core_trace::enable_tracing!();
