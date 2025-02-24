use crate::service::WritePoolRequest;
use fuel_core_services::stream::BoxStream;
use fuel_core_types::services::{
    block_importer::SharedImportResult,
    p2p::{
        PeerId,
        TransactionGossipData,
    },
};
use tokio::sync::mpsc;

/// Stores all subscriptions for the `TxPool` service.
pub(super) struct Subscriptions {
    pub new_tx: BoxStream<TransactionGossipData>,
    pub new_tx_source: BoxStream<PeerId>,
    pub imported_blocks: BoxStream<SharedImportResult>,
    pub write_pool: mpsc::Receiver<WritePoolRequest>,
}
