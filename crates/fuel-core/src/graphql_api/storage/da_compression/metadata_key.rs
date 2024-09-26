use strum::EnumCount;

/// The metadata key used by `DaCompressionTemporalRegistryMetadata` table to
/// store progress of the evictor.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    strum::EnumCount,
)]
pub enum MetadataKey {
    Address,
    AssetId,
    ContractId,
    ScriptCode,
    PredicateCode,
}

#[cfg(feature = "test-helpers")]
impl rand::distributions::Distribution<MetadataKey> for rand::distributions::Standard {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> MetadataKey {
        match rng.next_u32() as usize % MetadataKey::COUNT {
            0 => MetadataKey::Address,
            1 => MetadataKey::AssetId,
            2 => MetadataKey::ContractId,
            3 => MetadataKey::ScriptCode,
            4 => MetadataKey::PredicateCode,
            _ => unreachable!("New metadata key is added but not supported here"),
        }
    }
}
