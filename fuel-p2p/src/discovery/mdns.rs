use libp2p::{
    mdns::{
        MdnsConfig,
        MdnsEvent,
        TokioMdns,
    },
    swarm::{
        NetworkBehaviour,
        NetworkBehaviourAction,
        PollParameters,
    },
    Multiaddr,
    PeerId,
};
use std::task::{
    Context,
    Poll,
};
use tracing::warn;

#[allow(clippy::large_enum_variant)]
pub enum MdnsWrapper {
    Ready(TokioMdns),
    Disabled,
}

impl Default for MdnsWrapper {
    fn default() -> Self {
        match TokioMdns::new(MdnsConfig::default()) {
            Ok(mdns) => Self::Ready(mdns),
            Err(err) => {
                warn!("Failed to initialize mDNS: {:?}", err);
                Self::Disabled
            }
        }
    }
}

impl MdnsWrapper {
    pub fn disabled() -> Self {
        MdnsWrapper::Disabled
    }

    pub fn addresses_of_peer(&mut self, peer_id: &PeerId) -> Vec<Multiaddr> {
        match self {
            Self::Ready(mdns) => mdns.addresses_of_peer(peer_id),
            _ => Vec::new(),
        }
    }

    pub fn poll(
        &mut self,
        cx: &mut Context<'_>,
        params: &mut impl PollParameters,
    ) -> Poll<
        NetworkBehaviourAction<
            MdnsEvent,
            <TokioMdns as NetworkBehaviour>::ConnectionHandler,
        >,
    > {
        match self {
            Self::Ready(mdns) => mdns.poll(cx, params),
            Self::Disabled => Poll::Pending,
        }
    }
}
