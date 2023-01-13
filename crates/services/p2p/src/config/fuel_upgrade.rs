use futures::{
    AsyncRead,
    AsyncWrite,
    Future,
    FutureExt,
};
use libp2p::{
    InboundUpgrade,
    OutboundUpgrade,
};
use libp2p_core::{
    upgrade::{
        read_length_prefixed,
        write_length_prefixed,
    },
    UpgradeInfo,
};
use std::{
    error::Error,
    fmt,
    io,
    pin::Pin,
};

/// Sha256 hash of ChainConfig
#[derive(Debug, Clone, Copy, Default)]
pub struct Checksum([u8; 32]);

impl From<[u8; 32]> for Checksum {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}

/// When two nodes want to establish a connection they need to
/// exchange the Hash of their respective Chain Id and Chain Config.
/// The connection is only accepted if their hashes match.
/// This is used to aviod peers having same network name but different configurations connecting to each other.
#[derive(Debug, Clone)]
pub(crate) struct FuelUpgrade {
    checksum: Checksum,
}

impl FuelUpgrade {
    pub(crate) fn new(checksum: Checksum) -> Self {
        Self { checksum }
    }
}

#[derive(Debug)]
pub(crate) enum FuelUpgradeError {
    IncorrectChecksum,
    Io(io::Error),
}

impl fmt::Display for FuelUpgradeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FuelUpgradeError::Io(e) => write!(f, "{}", e),            
            FuelUpgradeError::IncorrectChecksum => f.write_str("Fuel node checksum does not match, either ChainId or ChainConfig are not the same, or both."),            
        }
    }
}

impl From<io::Error> for FuelUpgradeError {
    fn from(e: io::Error) -> Self {
        FuelUpgradeError::Io(e)
    }
}

impl Error for FuelUpgradeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            FuelUpgradeError::Io(e) => Some(e),
            FuelUpgradeError::IncorrectChecksum => None,
        }
    }
}

impl UpgradeInfo for FuelUpgrade {
    type Info = &'static [u8];
    type InfoIter = std::iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        std::iter::once(b"/fuel/upgrade/0")
    }
}

impl<C> InboundUpgrade<C> for FuelUpgrade
where
    C: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = C;
    type Error = FuelUpgradeError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, mut socket: C, _: Self::Info) -> Self::Future {
        async move {
            // Inbound node receives the checksum and compares it to its own checksum.
            // If they do not match the connection is rejected.
            let res = read_length_prefixed(&mut socket, self.checksum.0.len()).await?;
            if res != self.checksum.0 {
                return Err(FuelUpgradeError::IncorrectChecksum)
            }

            Ok(socket)
        }
        .boxed()
    }
}

impl<C> OutboundUpgrade<C> for FuelUpgrade
where
    C: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = C;
    type Error = FuelUpgradeError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, mut socket: C, _: Self::Info) -> Self::Future {
        async move {
            // Outbound node sends their own checksum for comparison with the inbound node.
            write_length_prefixed(&mut socket, &self.checksum.0).await?;

            // Note: outbound node does not need to receive the checksum from the inbound node,
            // since inbound node will reject the connection if the two don't match on its side.

            Ok(socket)
        }
        .boxed()
    }
}
