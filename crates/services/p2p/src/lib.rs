#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]

pub mod behavior;
pub mod codecs;
pub mod config;
pub mod discovery;
mod dnsaddr_resolution;
pub mod gossipsub;
pub mod heartbeat;
pub mod heavy_task_processor;
pub mod p2p_service;
pub mod peer_manager;
pub mod peer_report;
pub mod ports;
pub mod request_response;
pub mod service;
mod utils;

pub use gossipsub::config as gossipsub_config;
pub use heartbeat::Config;

pub use libp2p::{
    multiaddr::Protocol,
    Multiaddr,
    PeerId,
};
use tracing::warn;

#[cfg(feature = "test-helpers")]
pub mod network_service {
    pub use crate::p2p_service::*;
}

pub trait TryPeerId {
    /// Tries convert `Self` into `PeerId`.
    fn try_to_peer_id(&self) -> Option<PeerId>;
}

impl TryPeerId for Multiaddr {
    fn try_to_peer_id(&self) -> Option<PeerId> {
        self.iter().last().and_then(|p| match p {
            Protocol::P2p(peer_id) => Some(peer_id),
            Protocol::Dnsaddr(multiaddr) => {
                warn!(
                    "synchronous recursive dnsaddr resolution is not yet supported: {:?}",
                    multiaddr
                );
                None
            }
            _ => None,
        })
    }
}

#[cfg(test)]
fuel_core_trace::enable_tracing!();
