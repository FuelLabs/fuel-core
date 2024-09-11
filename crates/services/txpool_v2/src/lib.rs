#![allow(dead_code)]
#![allow(unused)]

mod collision_manager;
mod config;
mod error;
mod pool;
mod ports;
mod selection_algorithms;
mod service;
mod storage;
mod transaction_conversion;

type GasPrice = Word;

#[cfg(test)]
mod tests;

use fuel_core_types::fuel_asm::Word;
pub use service::{
    new_service,
    Service,
    SharedState,
};
