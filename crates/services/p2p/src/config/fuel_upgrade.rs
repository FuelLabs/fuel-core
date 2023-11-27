use asynchronous_codec::{
    BytesCodec,
    FramedRead,
    FramedWrite,
};
use futures::{
    AsyncRead,
    AsyncWrite,
    Future,
    FutureExt,
    SinkExt,
    TryStreamExt,
};
use libp2p_core::{
    upgrade::{
        InboundConnectionUpgrade,
        OutboundConnectionUpgrade,
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
            FuelUpgradeError::Io(e) => write!(f, "{e}"),            
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
    type Info = &'static str;
    type InfoIter = std::iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        std::iter::once("/fuel/upgrade/0")
    }
}

fn invalid_data_err() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Invalid data in Fuel Upgrade code",
    )
}

impl<C> InboundConnectionUpgrade<C> for FuelUpgrade
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
            let res = FramedRead::new(&mut socket, BytesCodec)
                .try_next()
                .await?
                .ok_or(invalid_data_err())?;

            if res.as_ref() != self.checksum.0.as_ref() {
                return Err(FuelUpgradeError::IncorrectChecksum)
            }

            Ok(socket)
        }
        .boxed()
    }
}

impl<C> OutboundConnectionUpgrade<C> for FuelUpgrade
where
    C: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = C;
    type Error = FuelUpgradeError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, mut socket: C, _: Self::Info) -> Self::Future {
        async move {
            let mut framed = FramedWrite::new(&mut socket, BytesCodec);
            let bytes = self.checksum.0.to_vec().into();
            framed.send(bytes).await?;
            framed.close().await?;
            // // Note: outbound node does not need to receive the checksum from the inbound node,
            // // since inbound node will reject the connection if the two don't match on its side.
            //
            Ok(socket)
        }
        .boxed()
    }
}
