use crate::ListenerId;
use libp2p_core::Multiaddr;

#[derive(Debug)]
pub struct ListenOpts {
    id: ListenerId,
    address: Multiaddr,
}

impl ListenOpts {
    pub fn new(address: Multiaddr) -> ListenOpts {
        ListenOpts {
            id: ListenerId::next(),
            address,
        }
    }

    /// Get the [`ListenerId`] of this listen attempt
    pub fn listener_id(&self) -> ListenerId {
        self.id
    }

    /// Get the [`Multiaddr`] that is being listened on
    pub fn address(&self) -> &Multiaddr {
        &self.address
    }
}

impl From<Multiaddr> for ListenOpts {
    fn from(addr: Multiaddr) -> Self {
        ListenOpts::new(addr)
    }
}
