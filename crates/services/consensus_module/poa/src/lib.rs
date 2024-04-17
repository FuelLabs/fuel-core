#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(unused_must_use)]
#![deny(warnings)]

mod deadline_clock;
mod sync;

#[cfg(test)]
mod service_test;

pub mod config;
pub mod ports;
pub mod service;
pub mod verifier;

pub use config::{
    Config,
    Trigger,
};
pub use service::{
    new_service,
    Service,
};
