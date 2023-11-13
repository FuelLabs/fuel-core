#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

pub mod config;
pub mod importer;
pub mod ports;

pub use config::Config;
pub use importer::Importer;

#[cfg(test)]
fuel_core_trace::enable_tracing!();
