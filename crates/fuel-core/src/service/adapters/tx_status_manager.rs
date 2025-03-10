use fuel_core_services::stream::BoxStream;
use fuel_core_tx_status_manager::ports::P2PPreConfirmationGossipData;

use super::P2PAdapter;

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
