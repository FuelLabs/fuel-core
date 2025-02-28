use fuel_core_services::stream::BoxStream;
use fuel_core_txpool::ports::TxStatusManager;
use fuel_core_types::{
    fuel_asm::RegId,
    fuel_tx::{
        TransactionBuilder,
        TxId,
    },
    services::{
        p2p::TransactionStatusGossipData,
        txpool::TransactionStatus,
    },
};

use crate::service::FuelService;

use super::{
    P2PAdapter,
    TxStatusManagerAdapter,
};

impl TxStatusManager for TxStatusManagerAdapter {
    fn upsert_status(&mut self, tx_id: &TxId, tx_status: TransactionStatus) {
        self.service.upsert_status(tx_id, tx_status)

        // TODO[RC]: Broadcast status change similarly to how TxPool does it
        // self.shared_state
        // .tx_status_sender
        // .send_submitted(tx_id, Tai64::from_unix(duration));
    }

    fn status(&self, tx_id: &TxId) -> Option<&TransactionStatus> {
        self.service.status(tx_id)
    }
}

#[cfg(feature = "p2p")]
impl fuel_core_tx_status_manager::ports::P2PSubscriptions for P2PAdapter {
    type GossipedStatuses = TransactionStatusGossipData;

    fn gossiped_tx_statuses(&self) -> BoxStream<Self::GossipedStatuses> {
        use tokio_stream::{
            wrappers::BroadcastStream,
            StreamExt,
        };

        if let Some(service) = &self.service {
            Box::pin(
                BroadcastStream::new(service.subscribe_tx_status())
                    .filter_map(|result| result.ok()),
            )
        } else {
            fuel_core_services::stream::IntoBoxStream::into_boxed(tokio_stream::pending())
        }
    }
}

#[cfg(not(feature = "p2p"))]
impl fuel_core_tx_status_manager::ports::P2PSubscriptions for P2PAdapter {
    type GossipedStatuses = TransactionStatusGossipData;

    fn gossiped_tx_statuses(&self) -> BoxStream<Self::GossipedStatuses> {
        Box::pin(fuel_core_services::stream::pending())
    }
}
