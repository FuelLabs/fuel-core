use futures::FutureExt;
use libp2p::mdns::{MdnsConfig, MdnsEvent};
use libp2p::swarm::{NetworkBehaviour, NetworkBehaviourAction, PollParameters};
use libp2p::{mdns::Mdns, Multiaddr, PeerId};
use log::warn;
use std::task::{Context, Poll};

pub enum MdnsWorker {
    Instantiating(futures::future::BoxFuture<'static, std::io::Result<Mdns>>),
    Ready(Mdns),
    Disabled,
}

impl MdnsWorker {
    pub fn new() -> Self {
        MdnsWorker::Instantiating(Mdns::new(MdnsConfig::default()).boxed())
    }

    pub fn disabled() -> Self {
        MdnsWorker::Disabled
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
    ) -> Poll<NetworkBehaviourAction<MdnsEvent, <Mdns as NetworkBehaviour>::ProtocolsHandler>> {
        loop {
            match self {
                Self::Instantiating(fut) => {
                    *self = match futures::ready!(fut.as_mut().poll(cx)) {
                        Ok(mdns) => Self::Ready(mdns),
                        Err(err) => {
                            warn!("Failed to initialize mDNS: {:?}", err);
                            Self::Disabled
                        }
                    }
                }
                Self::Ready(mdns) => return mdns.poll(cx, params),
                Self::Disabled => return Poll::Pending,
            }
        }
    }
}
