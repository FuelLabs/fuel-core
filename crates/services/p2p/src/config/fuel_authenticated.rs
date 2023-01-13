use fuel_core_types::secrecy::Zeroize;
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
        NoiseAuthenticated,
        NoiseError,
        NoiseOutput,
        Protocol,
    },
    InboundUpgrade,
    OutboundUpgrade,
    PeerId,
};
use std::pin::Pin;

pub(crate) trait Approver {
    /// Allows Peer connection based on it's PeerId and the Approver's knowledge of the Connection State
    fn allow_peer(&self, peer_id: &PeerId) -> bool;
}

#[derive(Clone)]
pub(crate) struct FuelAuthenticated<A: Approver, P, C: Zeroize, R> {
    noise_authenticated: NoiseAuthenticated<P, C, R>,
    approver: A,
}

impl<A: Approver, P, C: Zeroize, R> FuelAuthenticated<A, P, C, R> {
    pub(crate) fn new(
        noise_authenticated: NoiseAuthenticated<P, C, R>,
        approver: A,
    ) -> Self {
        Self {
            noise_authenticated,
            approver,
        }
    }
}

impl<A: Approver, P, C: Zeroize, R> UpgradeInfo for FuelAuthenticated<A, P, C, R>
where
    NoiseAuthenticated<P, C, R>: UpgradeInfo,
{
    type Info = <NoiseAuthenticated<P, C, R> as UpgradeInfo>::Info;
    type InfoIter = <NoiseAuthenticated<P, C, R> as UpgradeInfo>::InfoIter;

    fn protocol_info(&self) -> Self::InfoIter {
        self.noise_authenticated.protocol_info()
    }
}

impl<A, T, P, C, R> InboundUpgrade<T> for FuelAuthenticated<A, P, C, R>
where
    NoiseAuthenticated<P, C, R>: UpgradeInfo
        + InboundUpgrade<T, Output = (PeerId, NoiseOutput<T>), Error = NoiseError>
        + 'static,
    <NoiseAuthenticated<P, C, R> as InboundUpgrade<T>>::Future: Send,
    T: AsyncRead + AsyncWrite + Send + 'static,
    C: Protocol<C> + AsRef<[u8]> + Zeroize + Send + 'static,
    A: Approver + Send + 'static,
{
    type Output = (PeerId, NoiseOutput<T>);
    type Error = NoiseError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, socket: T, info: Self::Info) -> Self::Future {
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

impl<A, T, P, C, R> OutboundUpgrade<T> for FuelAuthenticated<A, P, C, R>
where
    NoiseAuthenticated<P, C, R>: UpgradeInfo
        + OutboundUpgrade<T, Output = (PeerId, NoiseOutput<T>), Error = NoiseError>
        + 'static,
    <NoiseAuthenticated<P, C, R> as OutboundUpgrade<T>>::Future: Send,
    T: AsyncRead + AsyncWrite + Send + 'static,
    C: Protocol<C> + AsRef<[u8]> + Zeroize + Send + 'static,
    A: Approver + Send + 'static,
{
    type Output = (PeerId, NoiseOutput<T>);
    type Error = NoiseError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, socket: T, info: Self::Info) -> Self::Future {
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
