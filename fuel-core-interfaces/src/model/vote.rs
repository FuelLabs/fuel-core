use fuel_crypto::{PublicKey, Signature};
use fuel_types::Bytes32;

/// A vote from a validator.
///
/// This is a dummy placeholder for the Vote Struct in fuel-bft
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConsensusVote {
    block_id: Bytes32,
    height: u64,
    round: u64,
    signature: Signature,
    //step: Step,
    validator: PublicKey,
}
