use core::time::Duration;

use fuel_core_types::tai64::{Tai64, Tai64N};

#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// How long entries in the temporal registry are valid.
    /// After this time has passed, the entry is considered stale and must not be used.
    /// If the value is needed again, it must be re-registered.
    pub temporal_registry_retention: Duration,
}

impl Config {
    /// Given timestamp of the current block and a key in an older block,
    /// is the key is still accessible?
    /// Returns error if the arguments are not valid block timestamps,
    /// or if the block is older than the key.
    pub fn is_timestamp_accessible(
        &self,
        block_timestamp: Tai64,
        key_timestamp: Tai64,
    ) -> anyhow::Result<bool> {
        let block = Tai64N(block_timestamp, 0);
        let key = Tai64N(key_timestamp, 0);
        let duration = block
            .duration_since(&key)
            .map_err(|_| anyhow::anyhow!("Invalid timestamp ordering"))?;
        Ok(duration <= self.temporal_registry_retention)
    }
}
