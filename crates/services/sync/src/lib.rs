#![warn(unused_crate_dependencies)]

// pub mod config;
pub mod service;

// pub use config::Config;
// pub use service::Service;

use std::ops::Range;

use fuel_core_types::blockchain::primitives::BlockHeight;

pub mod import;
pub mod ports;
pub mod sync;

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
struct State {
    best_seen_height: Option<BlockHeight>,
    in_flight_height: Option<Range<BlockHeight>>,
}
