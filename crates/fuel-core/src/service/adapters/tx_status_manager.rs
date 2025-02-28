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
    fn add_status(&mut self, tx_id: &TxId, tx_status: &TransactionStatus) {
        self.service.add_status(tx_id, tx_status)
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

        todo!()

        // Box::pin(
        // BroadcastStream::new(self.service.subscribe_tx())
        // .filter_map(|result| result.ok()),
        // )
    }
}
