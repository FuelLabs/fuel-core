#![deny(unused_must_use)]
#![deny(warnings)]

mod balances;
mod blocks;
mod chain;
mod coin;
mod coins;
mod contract;
mod dap;
mod debugger;
mod deployment;
mod fee_collection_contract;

mod gas_price;
mod health;
mod helpers;
mod messages;
mod metrics;
mod node_info;
mod poa;
#[cfg(feature = "relayer")]
mod relayer;
mod snapshot;
#[cfg(feature = "p2p")]
mod sync;
mod trigger_integration;
mod tx;
#[cfg(feature = "p2p")]
mod tx_gossip;
mod vm_storage;

fuel_core_trace::enable_tracing!();
