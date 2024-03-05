mod chain;
mod coin;
mod consensus;
mod contract;
mod contract_balance;
mod contract_state;
mod message;
#[cfg(feature = "std")]
mod snapshot_metadata;
mod state;

#[cfg(all(test, feature = "random", feature = "std"))]
pub(crate) fn random_bytes_32(mut rng: impl rand::Rng) -> [u8; 32] {
    rng.gen()
}

#[cfg(all(test, feature = "random", feature = "std"))]
pub(crate) trait Randomize {
    fn randomize(rng: impl rand::Rng) -> Self;
}

pub use chain::*;
pub use coin::*;
pub use consensus::*;
pub use contract::*;
pub use contract_balance::*;
pub use contract_state::*;
pub use message::*;
#[cfg(feature = "std")]
pub use snapshot_metadata::*;
pub use state::*;
