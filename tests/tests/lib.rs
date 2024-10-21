#![deny(unused_must_use)]
#![allow(warnings)] // Tmp change

mod balances;
mod blob;
mod blocks;
mod chain;
mod coin;
mod coins;
mod contract;
mod da_compression;
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
#[cfg(feature = "relayer")]
mod relayer;
mod snapshot;
mod state_rewind;
#[cfg(feature = "p2p")]
mod sync;
mod trigger_integration;
mod tx;
#[cfg(feature = "p2p")]
mod tx_gossip;
mod vm_storage;

fuel_core_trace::enable_tracing!();
