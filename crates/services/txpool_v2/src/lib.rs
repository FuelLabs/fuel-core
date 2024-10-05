#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(warnings)]
#![allow(dead_code)]
#![allow(unused)]

#[cfg(feature = "compil")]
mod collision_manager;
#[cfg(feature = "compil")]
mod config;
#[cfg(feature = "compil")]
mod error;
#[cfg(feature = "compil")]
mod heavy_async_processing;
#[cfg(feature = "compil")]
mod pool;
#[cfg(feature = "compil")]
mod ports;
#[cfg(feature = "compil")]
mod selection_algorithms;
#[cfg(feature = "compil")]
mod service;
#[cfg(feature = "compil")]
mod storage;
#[cfg(feature = "compil")]
mod verifications;

#[cfg(feature = "compil")]
#[cfg(test)]
mod tests;
#[cfg(test)]
fuel_core_trace::enable_tracing!();

#[cfg(feature = "compil")]
use fuel_core_types::fuel_asm::Word;
#[cfg(feature = "compil")]
pub use service::{
    new_service,
    Service,
    SharedState,
};
