use super::{
    fuel_authenticated::Approver,
    peer_ids_set_from,
};
use crate::peer_manager::ConnectionState;
use fuel_core_services::seqlock::SeqLockReader;
use libp2p::{
    Multiaddr,
    PeerId,
};
use std::collections::HashSet;

/// A `ConnectionTracker` allows either Reserved Peers or other peers if there is an available slot.
/// It is synced with `PeerManager` which keeps track of the `ConnectionState`.
#[derive(Debug, Clone)]
pub(crate) struct ConnectionTracker {
    reserved_nodes: HashSet<PeerId>,
    connection_state: Option<SeqLockReader<ConnectionState>>,
}

impl ConnectionTracker {
    pub(crate) fn new(
        reserved_nodes: &[Multiaddr],
        connection_state: Option<SeqLockReader<ConnectionState>>,
    ) -> Self {
        Self {
            reserved_nodes: peer_ids_set_from(reserved_nodes),
            connection_state,
        }
    }
}

impl Approver for ConnectionTracker {
    fn allow_peer(&self, peer_id: &PeerId) -> bool {
        if self.reserved_nodes.contains(peer_id) {
            return true
        }

        if let Some(connection_state) = &self.connection_state {
            return connection_state.read().available_slot();
        }

        false
    }
}
