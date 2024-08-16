#![deny(unused_must_use)]
#![deny(warnings)]

mod balances;
mod blob;
mod blocks;
mod chain;
mod coin;

#[cfg(feature = "p2p")]
#[cfg(feature = "p2p")]
#[cfg(feature = "relayer")]
fuel_core_trace::enable_tracing!();
mod coins;
mod contract;
mod dap;
mod debugger;
mod dos;
mod fee_collection_contract;
mod gas_price;
mod health;
mod helpers;
mod local_node;
mod messages;
mod metrics;
mod node_info;
mod poa;
mod recovery;
mod regenesis;
mod relayer;
mod snapshot;
mod state_rewind;
mod sync;
mod trigger_integration;
mod tx;
mod tx_gossip;
mod vm_storage;
