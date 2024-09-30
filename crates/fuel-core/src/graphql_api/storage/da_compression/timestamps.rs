use fuel_core_types::fuel_compression::RegistryKey;

/// The metadata key used by `DaCompressionTemporalRegistryTimsetamps` table to
/// keep track of when each key was last updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TimestampKey {
    /// The column where the key is stored.
    pub keyspace: TimestampKeyspace,
    /// The key itself.
    pub key: RegistryKey,
}

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
pub enum TimestampKeyspace {
    Address,
    AssetId,
    ContractId,
    ScriptCode,
    PredicateCode,
}

#[cfg(feature = "test-helpers")]
impl rand::distributions::Distribution<TimestampKeyspace>
    for rand::distributions::Standard
{
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> TimestampKeyspace {
        use strum::EnumCount;
        match rng.next_u32() as usize % TimestampKeyspace::COUNT {
            0 => TimestampKeyspace::Address,
            1 => TimestampKeyspace::AssetId,
            2 => TimestampKeyspace::ContractId,
            3 => TimestampKeyspace::ScriptCode,
            4 => TimestampKeyspace::PredicateCode,
            _ => unreachable!("New metadata key is added but not supported here"),
        }
    }
}
