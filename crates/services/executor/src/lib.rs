#![cfg_attr(not(feature = "std"), no_std)]
#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod executor;
pub mod ports;
pub mod refs;

#[cfg(test)]
fuel_core_trace::enable_tracing!();
