use futures::{
    future::BoxFuture,
    FutureExt,
};
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
// Wrapper around mDNS so that `DiscoveryConfig::finish` does not have to be an `async` function
pub enum MdnsWrapper {
    Instantiating(BoxFuture<'static, std::io::Result<TokioMdns>>),
    Ready(TokioMdns),
    Disabled,
}

impl Default for MdnsWrapper {
    fn default() -> Self {
        MdnsWrapper::Instantiating(TokioMdns::new(MdnsConfig::default()).boxed())
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
