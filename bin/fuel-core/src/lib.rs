#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

pub mod cli;
pub use fuel_core::service::FuelService;

use fuel_core_block_aggregator_api as _;
use tikv_jemallocator as _; // Used only by the binary
