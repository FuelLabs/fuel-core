#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

mod collision_manager;
mod config;
mod error;
mod pool;
mod ports;
mod selection_algorithms;
mod service;
mod shared_state;
mod storage;
mod tx_status_stream;
mod update_sender;

type GasPrice = Word;

#[cfg(test)]
mod tests;
#[cfg(test)]
fuel_core_trace::enable_tracing!();

use fuel_core_types::fuel_asm::Word;
pub use service::{
    new_service,
    Service,
};
pub use shared_state::SharedState;
