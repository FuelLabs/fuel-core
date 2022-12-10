pub mod config;
mod serialization;

pub use config::*;
use fuel_core_interfaces::common::prelude::MerkleRoot;

pub trait GenesisCommitment {
    /// Calculates the merkle root of the state of the entity.
    fn root(&mut self) -> anyhow::Result<MerkleRoot>;
}
