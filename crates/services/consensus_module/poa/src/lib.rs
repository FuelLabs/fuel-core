#![deny(unused_must_use)]

mod deadline_clock;

pub mod config;
pub mod ports;
pub mod service;

pub use config::{
    Config,
    Trigger,
};
pub use service::Service;
