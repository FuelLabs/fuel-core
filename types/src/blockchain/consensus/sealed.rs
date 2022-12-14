use crate::blockchain::consensus::Consensus;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
/// A sealed entity with consensus info.
pub struct Sealed<Entity> {
    pub entity: Entity,
    pub consensus: Consensus,
}
