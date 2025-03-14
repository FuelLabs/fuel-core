use super::P2PAdapter;
use fuel_core_chain_config::ConsensusConfig;
use fuel_core_services::stream::BoxStream;
use fuel_core_tx_status_manager::{
    ports::P2PPreConfirmationGossipData,
    service::ProtocolPublicKey,
};
use fuel_core_types::fuel_tx::Address;

#[cfg(feature = "p2p")]
impl fuel_core_tx_status_manager::ports::P2PSubscriptions for P2PAdapter {
    type GossipedStatuses = P2PPreConfirmationGossipData;

    fn gossiped_tx_statuses(&self) -> BoxStream<Self::GossipedStatuses> {
        use tokio_stream::{
            wrappers::BroadcastStream,
            StreamExt,
        };

        if let Some(service) = &self.service {
            Box::pin(
                BroadcastStream::new(service.subscribe_preconfirmations())
                    .filter_map(|result| result.ok()),
            )
        } else {
            fuel_core_services::stream::IntoBoxStream::into_boxed(tokio_stream::pending())
        }
    }
}

#[cfg(not(feature = "p2p"))]
impl fuel_core_tx_status_manager::ports::P2PSubscriptions for P2PAdapter {
    type GossipedStatuses = P2PPreConfirmationGossipData;

    fn gossiped_tx_statuses(&self) -> BoxStream<Self::GossipedStatuses> {
        Box::pin(fuel_core_services::stream::pending())
    }
}

pub struct ConsensusConfigProtocolPublicKey {
    inner: ConsensusConfig,
}

impl ConsensusConfigProtocolPublicKey {
    pub fn new(inner: ConsensusConfig) -> Self {
        Self { inner }
    }
}

impl ProtocolPublicKey for ConsensusConfigProtocolPublicKey {
    fn latest_address(&self) -> Address {
        match &self.inner {
            ConsensusConfig::PoA { signing_key } => *signing_key,
            ConsensusConfig::PoAV2(poa_v2) => poa_v2.latest_address(),
        }
    }
}
