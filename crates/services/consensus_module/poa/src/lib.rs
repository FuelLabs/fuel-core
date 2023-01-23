#![deny(unused_crate_dependencies)]
#![deny(unused_must_use)]

mod deadline_clock;

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
