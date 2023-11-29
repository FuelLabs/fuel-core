use futures::{
    future,
    AsyncRead,
    AsyncWrite,
    Future,
    TryFutureExt,
};
use libp2p::{
    core::UpgradeInfo,
    noise::{
        Config as NoiseConfig,
        Error as NoiseError,
        Output as NoiseOutput,
    },
    PeerId,
};
use libp2p_core::upgrade::{
    InboundConnectionUpgrade,
    OutboundConnectionUpgrade,
};
use std::pin::Pin;

pub(crate) trait Approver {
    /// Allows Peer connection based on it's PeerId and the Approver's knowledge of the Connection State
    fn allow_peer(&self, peer_id: &PeerId) -> bool;
}

#[derive(Clone)]
pub(crate) struct FuelAuthenticated<A: Approver> {
    noise_authenticated: NoiseConfig,
    approver: A,
}

impl<A: Approver> FuelAuthenticated<A> {
    pub(crate) fn new(noise_authenticated: NoiseConfig, approver: A) -> Self {
        Self {
            noise_authenticated,
            approver,
        }
    }
}

impl<A: Approver> UpgradeInfo for FuelAuthenticated<A> {
    type Info = <NoiseConfig as UpgradeInfo>::Info;
    type InfoIter = <NoiseConfig as UpgradeInfo>::InfoIter;

    fn protocol_info(&self) -> Self::InfoIter {
        self.noise_authenticated.protocol_info()
    }
}

impl<A, T> InboundConnectionUpgrade<T> for FuelAuthenticated<A>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    A: Approver + Send + 'static,
{
    type Output = (PeerId, NoiseOutput<T>);
    type Error = NoiseError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, socket: T, info: Self::Info) -> Self::Future {
        tracing::error!("auth inbound!");
        Box::pin(
            self.noise_authenticated
                .upgrade_inbound(socket, info)
                .and_then(move |(remote_peer_id, io)| {
                    if self.approver.allow_peer(&remote_peer_id) {
                        future::ok((remote_peer_id, io))
                    } else {
                        future::err(NoiseError::AuthenticationFailed)
                    }
                }),
        )
    }
}

impl<A, T> OutboundConnectionUpgrade<T> for FuelAuthenticated<A>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    A: Approver + Send + 'static,
{
    type Output = (PeerId, NoiseOutput<T>);
    type Error = NoiseError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, socket: T, info: Self::Info) -> Self::Future {
        tracing::error!("auth outbound!");
        Box::pin(
            self.noise_authenticated
                .upgrade_outbound(socket, info)
                .and_then(move |(remote_peer_id, io)| {
                    if self.approver.allow_peer(&remote_peer_id) {
                        future::ok((remote_peer_id, io))
                    } else {
                        future::err(NoiseError::AuthenticationFailed)
                    }
                }),
        )
    }
}
