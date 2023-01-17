use super::{
    fuel_authenticated::Approver,
    peer_ids_set_from,
};
use libp2p::{
    Multiaddr,
    PeerId,
};
use std::collections::HashSet;

/// A `GuardedNode` only accepts connections from the set of the reserved nodes
#[derive(Debug, Clone)]
pub(crate) struct GuardedNode {
    reserved_nodes: HashSet<PeerId>,
}

impl GuardedNode {
    pub(crate) fn new(reserved_nodes: &[Multiaddr]) -> Self {
        Self {
            reserved_nodes: peer_ids_set_from(reserved_nodes),
        }
    }
}

impl Approver for GuardedNode {
    /// Checks if PeerId of the remote node is contained within the reserved nodes.
    /// It rejects the connection otherwise.
    fn allow_peer(&self, peer_id: &PeerId) -> bool {
        self.reserved_nodes.contains(peer_id)
    }
}
