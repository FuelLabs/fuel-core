use libp2p::{
    mdns::{
        tokio::Behaviour as TokioMdns,
        Config,
        Event as MdnsEvent,
    },
    swarm::{
        NetworkBehaviour,
        ToSwarm,
    },
    PeerId,
};
use libp2p_swarm::THandlerInEvent;
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

impl MdnsWrapper {
    pub fn new(peer_id: PeerId) -> Self {
        match TokioMdns::new(Config::default(), peer_id) {
            Ok(mdns) => Self::Ready(mdns),
            Err(err) => {
                warn!("Failed to initialize mDNS: {:?}", err);
                Self::Disabled
            }
        }
    }

    pub fn disabled() -> Self {
        MdnsWrapper::Disabled
    }

    pub fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<MdnsEvent, THandlerInEvent<TokioMdns>>> {
        match self {
            Self::Ready(mdns) => mdns.poll(cx, params),
            Self::Disabled => Poll::Pending,
        }
    }
}
