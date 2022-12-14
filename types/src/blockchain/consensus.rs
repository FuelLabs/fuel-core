mod consensus;
mod genesis;
mod poa;
mod sealed;
mod vote;

pub use consensus::Consensus;
pub use genesis::Genesis;
pub use poa::PoAConsensus;
pub use sealed::Sealed;
pub use vote::ConsensusVote;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsensusType {
    PoA,
}
