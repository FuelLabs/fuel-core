mod behavior;
mod codecs;
pub mod config;
mod discovery;
mod gossipsub;
mod heartbeat;
mod p2p_service;
mod peer_manager;
pub mod ports;
mod request_response;
pub mod service;

pub use gossipsub::config as gossipsub_config;

pub use libp2p::{
    Multiaddr,
    PeerId,
};

#[cfg(test)]
fuel_core_trace::enable_tracing!();
