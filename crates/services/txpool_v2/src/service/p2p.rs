use crate::{
    error::Error,
    ports::P2PRequests,
};
use fuel_core_types::services::{
    p2p::{
        GossipsubMessageAcceptance,
        GossipsubMessageInfo,
    },
    txpool::PoolTransaction,
};
use std::sync::Arc;

pub trait P2PExt {
    fn process_insertion_result(
        &self,
        from_peer_info: Option<GossipsubMessageInfo>,
        result: &Result<PoolTransaction, Error>,
    );
}

impl P2PExt for Arc<dyn P2PRequests> {
    fn process_insertion_result(
        &self,
        from_peer_info: Option<GossipsubMessageInfo>,
        result: &Result<PoolTransaction, Error>,
    ) {
        if let Some(from_peer_info) = from_peer_info {
            match &result {
                Ok(_) => {
                    let _ = self.notify_gossip_transaction_validity(
                        from_peer_info,
                        GossipsubMessageAcceptance::Accept,
                    );
                }
                Err(Error::ConsensusValidity(_)) | Err(Error::MintIsDisallowed) => {
                    let _ = self.notify_gossip_transaction_validity(
                        from_peer_info,
                        GossipsubMessageAcceptance::Reject,
                    );
                }
                Err(_) => {
                    let _ = self.notify_gossip_transaction_validity(
                        from_peer_info,
                        GossipsubMessageAcceptance::Ignore,
                    );
                }
            };
        } else {
            if let Ok(transaction) = &result {
                let tx_clone = Arc::new(transaction.into());
                if let Err(e) = self.broadcast_transaction(tx_clone) {
                    tracing::error!("Failed to broadcast transaction: {}", e);
                }
            }
        }
    }
}
