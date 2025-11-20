pub mod provider;
mod transport;

use crate::quorum::transport::WeightedTransport;
use alloy_json_rpc::ResponsePacket;
use alloy_provider::transport::TransportError;
use std::{
    num::{NonZeroU8, NonZeroU64, NonZeroUsize},
    sync::Arc,
};
use thiserror::Error;

#[derive(Debug, Default, Copy, Clone)]
pub enum Quorum {
    ///  The quorum is reached when all providers return the exact value
    All,
    /// The quorum is reached when the majority of the providers have returned a
    /// matching value, taking into account their weight.
    #[default]
    Majority,
    /// The quorum is reached when the cumulative weight of a matching return
    /// exceeds the given percentage of the total weight.
    ///
    /// NOTE: this must be less than `100u8`
    Percentage(NonZeroU8),
    /// The quorum is reached when the given number of provider agree
    /// The configured weight is ignored in this case.
    ProviderCount(NonZeroUsize),
    /// The quorum is reached once the accumulated weight of the matching return
    /// exceeds this weight.
    Weight(NonZeroU64),
}

impl Quorum {
    fn weight(self, providers: &[WeightedTransport]) -> u64 {
        match self {
            Quorum::All => providers.iter().map(|p| p.weight).sum::<u64>(),
            Quorum::Majority => {
                let total = providers.iter().map(|p| p.weight).sum::<u64>();
                let rem = total % 2;
                total / 2 + rem
            }
            Quorum::Percentage(p) => {
                providers.iter().map(|p| p.weight).sum::<u64>() * (p.get() as u64) / 100
            }
            Quorum::ProviderCount(num) => {
                // take the lowest `num` weights
                let mut weights = providers.iter().map(|p| p.weight).collect::<Vec<_>>();
                weights.sort_unstable();
                weights.into_iter().take(num.get()).sum()
            }
            Quorum::Weight(w) => w.get(),
        }
    }
}

#[derive(Error, Debug)]
/// Error thrown when sending an HTTP request
pub enum QuorumError {
    #[error("No Quorum reached.")]
    /// NoQuorumReached
    NoQuorumReached {
        /// Returned responses
        values: Arc<[ResponsePacket]>,
        /// Returned errors
        errors: Arc<[TransportError]>,
    },
}
