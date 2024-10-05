#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(warnings)]
#![allow(dead_code)]
#![allow(unused)]

mod collision_manager;
mod config;
mod error;
mod heavy_async_processing;
mod pool;
mod ports;
mod selection_algorithms;
mod service;
mod storage;
mod verifications;

type GasPrice = Word;

#[cfg(test)]
mod tests;
#[cfg(test)]
fuel_core_trace::enable_tracing!();

use fuel_core_types::fuel_asm::Word;
pub use service::{
    new_service,
    Service,
    SharedState,
};
