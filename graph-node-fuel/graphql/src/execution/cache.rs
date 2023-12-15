use futures03::future::FutureExt;
use futures03::future::Shared;
use graph::{
    prelude::{debug, futures03, BlockPtr, CheapClone, Logger, QueryResult},
    util::timed_rw_lock::TimedMutex,
};
use stable_hash_legacy::crypto::SetHasher;
use stable_hash_legacy::prelude::*;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::{collections::HashMap, time::Duration};
use std::{
    collections::{hash_map::Entry, VecDeque},
    time::Instant,
};

use super::QueryHash;

type Hash = <SetHasher as StableHasher>::Out;

type PinFut<R> = Pin<Box<dyn Future<Output = R> + 'static + Send>>;
/// Cache that keeps a result around as long as it is still being processed.
/// The cache ensures that the query is not re-entrant, so multiple consumers
/// of identical queries will not execute them in parallel.
///
/// This has a lot in common with AsyncCache in the network-services repo,
/// but more specialized.
pub struct QueryCache<R> {
    cache: Arc<TimedMutex<HashMap<Hash, Shared<PinFut<R>>>>>,
}

impl<R: CheapClone> QueryCache<R> {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            cache: Arc::new(TimedMutex::new(HashMap::new(), id)),
        }
    }

    /// Assumption: Whatever F is passed in consistently returns the same
    /// value for any input - for all values of F used with this Cache.
    ///
    /// Returns `(value, cached)`, where `cached` is true if the value was
    /// already in the cache and false otherwise.
    pub async fn cached_query<F: Future<Output = R> + Send + 'static>(
        &self,
        hash: Hash,
        f: F,
        logger: &Logger,
    ) -> (R, bool) {
        let f = f.boxed();

        let (work, cached) = {
            let mut cache = self.cache.lock(logger);

            match cache.entry(hash) {
                Entry::Occupied(entry) => {
                    // This is already being worked on.
                    let entry = entry.get().cheap_clone();
                    (entry, true)
                }
                Entry::Vacant(entry) => {
                    // New work, put it in the in-flight list.
                    let uncached = f.shared();
                    entry.insert(uncached.clone());
                    (uncached, false)
                }
            }
        };

        let _remove_guard = if !cached {
            // Make sure to remove this from the in-flight list, even if `poll` panics.
            Some(defer::defer(|| {
                self.cache.lock(logger).remove(&hash);
            }))
        } else {
            None
        };

        (work.await, cached)
    }
}

#[derive(Debug)]
struct CacheByBlock {
    block: BlockPtr,
    max_weight: usize,
    weight: usize,

    // The value is `(result, n_hits)`.
    cache: HashMap<QueryHash, (Arc<QueryResult>, AtomicU64)>,
    total_insert_time: Duration,
}

impl CacheByBlock {
    fn new(block: BlockPtr, max_weight: usize) -> Self {
        CacheByBlock {
            block,
            max_weight,
            weight: 0,
            cache: HashMap::new(),
            total_insert_time: Duration::default(),
        }
    }

    fn get(&self, key: &QueryHash) -> Option<&Arc<QueryResult>> {
        let (value, hit_count) = self.cache.get(key)?;
        hit_count.fetch_add(1, Ordering::SeqCst);
        Some(value)
    }

    /// Returns `true` if the insert was successful or `false` if the cache was full.
    fn insert(&mut self, key: QueryHash, value: Arc<QueryResult>, weight: usize) -> bool {
        // We never try to insert errors into this cache, and always resolve some value.
        assert!(!value.has_errors());
        let fits_in_cache = self.weight + weight <= self.max_weight;
        if fits_in_cache {
            let start = Instant::now();
            self.weight += weight;
            self.cache.insert(key, (value, AtomicU64::new(0)));
            self.total_insert_time += start.elapsed();
        }
        fits_in_cache
    }
}

/// Organize block caches by network names. Since different networks
/// will be at different block heights, we need to keep their `CacheByBlock`
/// separate
pub struct QueryBlockCache {
    shard: u8,
    cache_by_network: Vec<(String, VecDeque<CacheByBlock>)>,
    max_weight: usize,
    max_blocks: usize,
}

impl QueryBlockCache {
    pub fn new(max_blocks: usize, shard: u8, max_weight: usize) -> Self {
        QueryBlockCache {
            shard,
            cache_by_network: Vec::new(),
            max_weight,
            max_blocks,
        }
    }

    pub fn insert(
        &mut self,
        network: &str,
        block_ptr: BlockPtr,
        key: QueryHash,
        result: Arc<QueryResult>,
        weight: usize,
        logger: Logger,
    ) -> bool {
        // Check if the cache is disabled
        if self.max_blocks == 0 {
            return false;
        }

        // Get or insert the cache for this network.
        let cache = match self
            .cache_by_network
            .iter_mut()
            .find(|(n, _)| n == network)
            .map(|(_, c)| c)
        {
            Some(c) => c,
            None => {
                self.cache_by_network
                    .push((network.to_owned(), VecDeque::new()));
                &mut self.cache_by_network.last_mut().unwrap().1
            }
        };

        // If there is already a cache by the block of this query, just add it there.
        if let Some(cache_by_block) = cache.iter_mut().find(|c| c.block == block_ptr) {
            return cache_by_block.insert(key, result.cheap_clone(), weight);
        }

        // We're creating a new `CacheByBlock` if:
        // - There are none yet, this is the first query being cached, or
        // - `block_ptr` is of higher or equal number than the most recent block in the cache.
        // Otherwise this is a historical query that does not belong in the block cache.
        if let Some(highest) = cache.iter().next() {
            if highest.block.number > block_ptr.number {
                return false;
            }
        };

        if cache.len() == self.max_blocks {
            // At capacity, so pop the oldest block.
            // Stats are reported in a task since we don't need the lock for it.
            let block = cache.pop_back().unwrap();
            let shard = self.shard;
            let network = network.to_string();

            graph::spawn(async move {
                let insert_time_ms = block.total_insert_time.as_millis();
                let mut dead_inserts = 0;
                let mut total_hits = 0;
                for (_, hits) in block.cache.values() {
                    let hits = hits.load(Ordering::SeqCst);
                    total_hits += hits;
                    if hits == 0 {
                        dead_inserts += 1;
                    }
                }
                let n_entries = block.cache.len();
                debug!(logger, "Rotating query cache, stats for last block";
                    "shard" => shard,
                    "network" => network,
                    "entries" => n_entries,
                    "avg_hits" => format!("{0:.2}", (total_hits as f64) / (n_entries as f64)),
                    "dead_inserts" => dead_inserts,
                    "fill_ratio" => format!("{0:.2}", (block.weight as f64) / (block.max_weight as f64)),
                    "avg_insert_time_ms" => format!("{0:.2}", insert_time_ms as f64 / (n_entries as f64)),
                )
            });
        }

        // Create a new cache by block, insert this entry, and add it to the QUERY_CACHE.
        let mut cache_by_block = CacheByBlock::new(block_ptr, self.max_weight);
        let cache_insert = cache_by_block.insert(key, result, weight);
        cache.push_front(cache_by_block);
        cache_insert
    }

    pub fn get(
        &self,
        network: &str,
        block_ptr: &BlockPtr,
        key: &QueryHash,
    ) -> Option<Arc<QueryResult>> {
        if let Some(cache) = self
            .cache_by_network
            .iter()
            .find(|(n, _)| n == network)
            .map(|(_, c)| c)
        {
            // Iterate from the most recent block looking for a block that matches.
            if let Some(cache_by_block) = cache.iter().find(|c| &c.block == block_ptr) {
                if let Some(response) = cache_by_block.get(key) {
                    return Some(response.cheap_clone());
                }
            }
        }
        None
    }
}
