#![deny(unused_must_use)]
#![deny(warnings)]

#[cfg(not(feature = "only-p2p"))]
mod balances;
#[cfg(not(feature = "only-p2p"))]
mod blob;
#[cfg(not(feature = "only-p2p"))]
mod blocks;
#[cfg(not(feature = "only-p2p"))]
mod chain;
#[cfg(not(feature = "only-p2p"))]
mod coin;
#[cfg(not(feature = "only-p2p"))]
mod coins;
#[cfg(not(feature = "only-p2p"))]
mod contract;
#[cfg(not(feature = "only-p2p"))]
mod da_compression;
#[cfg(not(feature = "only-p2p"))]
mod dap;
#[cfg(not(feature = "only-p2p"))]
mod debugger;
#[cfg(not(feature = "only-p2p"))]
mod dos;
#[cfg(not(feature = "only-p2p"))]
mod fee_collection_contract;
#[cfg(not(feature = "only-p2p"))]
mod gas_price;
#[cfg(not(feature = "only-p2p"))]
mod health;
#[cfg(not(feature = "only-p2p"))]
mod helpers;
#[cfg(not(feature = "only-p2p"))]
mod local_node;
#[cfg(not(feature = "only-p2p"))]
mod messages;
#[cfg(not(feature = "only-p2p"))]
mod metrics;
#[cfg(not(feature = "only-p2p"))]
mod node_info;
#[cfg(not(feature = "only-p2p"))]
mod poa;
#[cfg(not(feature = "only-p2p"))]
mod recovery;
#[cfg(not(feature = "only-p2p"))]
mod regenesis;
#[cfg(not(feature = "only-p2p"))]
mod relayer;
#[cfg(not(feature = "only-p2p"))]
mod snapshot;
#[cfg(not(feature = "only-p2p"))]
mod state_rewind;
#[cfg(not(feature = "only-p2p"))]
mod trigger_integration;
#[cfg(not(feature = "only-p2p"))]
mod tx;
#[cfg(not(feature = "only-p2p"))]
mod vm_storage;

#[cfg(feature = "only-p2p")]
mod sync;
#[cfg(feature = "only-p2p")]
mod tx_gossip;

#[cfg(feature = "aws-kms")]
mod aws_kms;

fuel_core_trace::enable_tracing!();
