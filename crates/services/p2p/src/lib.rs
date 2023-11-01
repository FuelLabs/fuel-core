#![deny(clippy::cast_possible_truncation)]

pub mod behavior;
pub mod codecs;
pub mod config;
pub mod discovery;
pub mod gossipsub;
pub mod heartbeat;
pub mod p2p_service;
pub mod peer_manager;
pub mod peer_report;
pub mod ports;
pub mod request_response;
pub mod service;

pub use gossipsub::config as gossipsub_config;
pub use heartbeat::HeartbeatConfig;

pub use libp2p::{
    multiaddr::Protocol,
    Multiaddr,
    PeerId,
};

#[cfg(feature = "test-helpers")]
pub mod network_service {
    pub use crate::p2p_service::*;
}

#[cfg(test)]
fuel_core_trace::enable_tracing!();
