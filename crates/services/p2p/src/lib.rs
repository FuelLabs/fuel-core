mod behavior;
pub mod codecs;
pub mod config;
mod discovery;
mod gossipsub;
mod heartbeat;
mod p2p_service;
mod peer_manager;
mod peer_report;
pub mod ports;
mod request_response;
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
