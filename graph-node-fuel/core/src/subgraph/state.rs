use graph::{
    prelude::{BlockPtr, Entity},
    schema::EntityKey,
    util::{backoff::ExponentialBackoff, lfu_cache::LfuCache},
};
use std::time::Instant;

pub struct IndexingState {
    /// `true` -> `false` on the first run
    pub should_try_unfail_non_deterministic: bool,
    /// `false` -> `true` once it reaches chain head
    pub synced: bool,
    /// Backoff used for the retry mechanism on non-deterministic errors
    pub backoff: ExponentialBackoff,
    /// Related to field above `backoff`
    ///
    /// Resets to `Instant::now` every time:
    /// - The time THRESHOLD is passed
    /// - Or the subgraph has triggers for the block
    pub skip_ptr_updates_timer: Instant,
    pub entity_lfu_cache: LfuCache<EntityKey, Option<Entity>>,
    pub cached_head_ptr: Option<BlockPtr>,
}
