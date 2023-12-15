use crate::env::ENV_VARS;
use crate::prelude::CacheWeight;
use priority_queue::PriorityQueue;
use std::cmp::Reverse;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

// The number of `evict` calls without access after which an entry is considered stale.
const STALE_PERIOD: u64 = 100;

/// `PartialEq` and `Hash` are delegated to the `key`.
#[derive(Clone, Debug)]
pub struct CacheEntry<K, V> {
    weight: usize,
    key: K,
    value: V,
    will_stale: bool,
}

impl<K: Eq, V> PartialEq for CacheEntry<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.key.eq(&other.key)
    }
}

impl<K: Eq, V> Eq for CacheEntry<K, V> {}

impl<K: Hash, V> Hash for CacheEntry<K, V> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key.hash(state)
    }
}

impl<K, V: Default + CacheWeight> CacheEntry<K, V> {
    fn cache_key(key: K) -> Self {
        // Only the key matters for finding an entry in the cache.
        CacheEntry {
            key,
            value: V::default(),
            weight: 0,
            will_stale: false,
        }
    }
}

impl<K: CacheWeight, V: Default + CacheWeight> CacheEntry<K, V> {
    /// Estimate the size of a `CacheEntry` with the given key and value. Do
    /// not count the size of `Self` since that is memory that is not freed
    /// when the cache entry is dropped as its storage is embedded in the
    /// `PriorityQueue`
    fn weight(key: &K, value: &V) -> usize {
        value.indirect_weight() + key.indirect_weight()
    }
}

// The priorities are `(stale, frequency)` tuples, first all stale entries will be popped and
// then non-stale entries by least frequency.
type Priority = (bool, Reverse<u64>);

/// Statistics about what happened during cache eviction
pub struct EvictStats {
    /// The weight of the cache after eviction
    pub new_weight: usize,
    /// The weight of the items that were evicted
    pub evicted_weight: usize,
    /// The number of entries after eviction
    pub new_count: usize,
    /// The number if entries that were evicted
    pub evicted_count: usize,
    /// Whether we updated the stale status of entries
    pub stale_update: bool,
    /// How long eviction took
    pub evict_time: Duration,
    /// The total number of cache accesses during this stale period
    pub accesses: usize,
    /// The total number of cache hits during this stale period
    pub hits: usize,
}

impl EvictStats {
    /// The cache hit rate in percent. The underlying counters are reset at
    /// the end of each stale period.
    pub fn hit_rate_pct(&self) -> f64 {
        if self.accesses > 0 {
            self.hits as f64 / self.accesses as f64 * 100.0
        } else {
            100.0
        }
    }
}
/// Each entry in the cache has a frequency, which is incremented by 1 on access. Entries also have
/// a weight, upon eviction first stale entries will be removed and then non-stale entries by order
/// of least frequency until the max weight is respected. This cache only removes entries on calls
/// to `evict`, so the max weight may be exceeded until `evict` is called. Every STALE_PERIOD
/// evictions entities are checked for staleness.
#[derive(Debug)]
pub struct LfuCache<K: Eq + Hash, V> {
    queue: PriorityQueue<CacheEntry<K, V>, Priority>,
    total_weight: usize,
    stale_counter: u64,
    dead_weight: bool,
    accesses: usize,
    hits: usize,
}

impl<K: Ord + Eq + Hash, V> Default for LfuCache<K, V> {
    fn default() -> Self {
        LfuCache {
            queue: PriorityQueue::new(),
            total_weight: 0,
            stale_counter: 0,
            dead_weight: false,
            accesses: 0,
            hits: 0,
        }
    }
}

impl<K: Clone + Ord + Eq + Hash + Debug + CacheWeight, V: CacheWeight + Default> LfuCache<K, V> {
    pub fn new() -> Self {
        LfuCache {
            queue: PriorityQueue::new(),
            total_weight: 0,
            stale_counter: 0,
            dead_weight: ENV_VARS.mappings.entity_cache_dead_weight,
            accesses: 0,
            hits: 0,
        }
    }

    /// Updates and bumps freceny if already present.
    pub fn insert(&mut self, key: K, value: V) {
        let weight = CacheEntry::weight(&key, &value);
        match self.get_mut(key.clone()) {
            None => {
                self.total_weight += weight;
                self.queue.push(
                    CacheEntry {
                        weight,
                        key,
                        value,
                        will_stale: false,
                    },
                    (false, Reverse(1)),
                );
            }
            Some(entry) => {
                let old_weight = entry.weight;
                entry.weight = weight;
                entry.value = value;
                self.total_weight -= old_weight;
                self.total_weight += weight;
            }
        }
    }

    #[cfg(test)]
    fn weight(&self, key: K) -> usize {
        let key_entry = CacheEntry::cache_key(key);
        self.queue
            .get(&key_entry)
            .map(|(entry, _)| entry.weight)
            .unwrap_or(0)
    }

    fn get_mut(&mut self, key: K) -> Option<&mut CacheEntry<K, V>> {
        // Increment the frequency by 1
        let key_entry = CacheEntry::cache_key(key);
        self.queue
            .change_priority_by(&key_entry, |(s, Reverse(f))| (s, Reverse(f + 1)));
        self.accesses += 1;
        self.queue.get_mut(&key_entry).map(|x| {
            self.hits += 1;
            x.0.will_stale = false;
            x.0
        })
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item = (&K, &V)> {
        self.queue
            .iter()
            .map(|entry| (&entry.0.key, &entry.0.value))
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.get_mut(key.clone()).map(|x| &x.value)
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        // `PriorityQueue` doesn't have a remove method, so emulate that by setting the priority to
        // the absolute minimum and popping.
        let key_entry = CacheEntry::cache_key(key.clone());
        self.queue
            .change_priority(&key_entry, (true, Reverse(u64::min_value())))
            .and_then(|_| {
                self.queue.pop().map(|(e, _)| {
                    assert_eq!(e.key, key_entry.key);
                    self.total_weight -= e.weight;
                    e.value
                })
            })
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.queue
            .get(&CacheEntry::cache_key(key.clone()))
            .is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn evict_and_stats(&mut self, max_weight: usize) -> EvictStats {
        self.evict_with_period(max_weight, STALE_PERIOD)
            .unwrap_or_else(|| EvictStats {
                new_weight: self.total_weight,
                evicted_weight: 0,
                new_count: self.len(),
                evicted_count: 0,
                stale_update: false,
                evict_time: Duration::from_millis(0),
                accesses: 0,
                hits: 0,
            })
    }

    /// Same as `evict_with_period(max_weight, STALE_PERIOD)`
    pub fn evict(&mut self, max_weight: usize) -> Option<EvictStats> {
        self.evict_with_period(max_weight, STALE_PERIOD)
    }

    /// Evict entries in the cache until the total weight of the cache is
    /// equal to or smaller than `max_weight`.
    ///
    /// The return value is mostly useful for testing and diagnostics and can
    /// safely ignored in normal use. It gives the sum of the weight of all
    /// evicted entries, the weight before anything was evicted and the new
    /// total weight of the cache, in that order, if anything was evicted
    /// at all. If there was no reason to evict, `None` is returned.
    pub fn evict_with_period(
        &mut self,
        max_weight: usize,
        stale_period: u64,
    ) -> Option<EvictStats> {
        if self.total_weight <= max_weight {
            return None;
        }

        let start = Instant::now();

        let accesses = self.accesses;
        let hits = self.hits;

        self.stale_counter += 1;
        if self.stale_counter == stale_period {
            self.stale_counter = 0;

            self.accesses = 0;
            self.hits = 0;

            // Entries marked `will_stale` were not accessed in this period. Properly mark them as
            // stale in their priorities. Also mark all entities as `will_stale` for the _next_
            // period so that they will be marked stale next time unless they are updated or looked
            // up between now and then.
            for (e, p) in self.queue.iter_mut() {
                p.0 = e.will_stale;
                e.will_stale = true;
            }
        }

        let mut evicted = 0;
        let old_len = self.len();
        let dead_weight = if self.dead_weight {
            self.len() * (std::mem::size_of::<CacheEntry<K, V>>() + 40)
        } else {
            0
        };
        while self.total_weight + dead_weight > max_weight {
            let entry = self
                .queue
                .pop()
                .expect("empty cache but total_weight > max_weight")
                .0;
            evicted += entry.weight;
            self.total_weight -= entry.weight;
        }
        Some(EvictStats {
            new_weight: self.total_weight,
            evicted_weight: evicted,
            new_count: self.len(),
            evicted_count: old_len - self.len(),
            stale_update: self.stale_counter == 0,
            evict_time: start.elapsed(),
            accesses,
            hits,
        })
    }
}

impl<K: Ord + Eq + Hash + 'static, V: 'static> IntoIterator for LfuCache<K, V> {
    type Item = (CacheEntry<K, V>, Priority);
    type IntoIter = Box<dyn Iterator<Item = Self::Item>>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.queue.into_iter())
    }
}

impl<K: Ord + Eq + Hash, V> Extend<(CacheEntry<K, V>, Priority)> for LfuCache<K, V> {
    fn extend<T: IntoIterator<Item = (CacheEntry<K, V>, Priority)>>(&mut self, iter: T) {
        self.queue.extend(iter);
    }
}

#[test]
fn entity_lru_cache() {
    #[derive(Default, Debug, PartialEq, Eq)]
    struct Weight(usize);

    impl CacheWeight for Weight {
        fn weight(&self) -> usize {
            self.indirect_weight()
        }

        fn indirect_weight(&self) -> usize {
            self.0
        }
    }

    let mut cache: LfuCache<&'static str, Weight> = LfuCache::new();
    cache.insert("panda", Weight(2));
    cache.insert("cow", Weight(1));
    let panda_weight = cache.weight("panda");
    let cow_weight = cache.weight("cow");

    assert_eq!(cache.get(&"cow"), Some(&Weight(1)));
    assert_eq!(cache.get(&"panda"), Some(&Weight(2)));

    // Nothing is evicted.
    cache.evict(panda_weight + cow_weight);
    assert_eq!(cache.len(), 2);

    // "cow" was accessed twice, so "panda" is evicted.
    cache.get(&"cow");
    cache.evict(cow_weight);
    assert!(cache.get(&"panda").is_none());

    cache.insert("alligator", Weight(2));
    let alligator_weight = cache.weight("alligator");

    // Give "cow" and "alligator" a high frequency.
    for _ in 0..1000 {
        cache.get(&"cow");
        cache.get(&"alligator");
    }

    // Insert a lion and make it weigh the same as the cow and the alligator
    // together.
    cache.insert("lion", Weight(0));
    let lion_weight = cache.weight("lion");
    let lion_inner_weight = cow_weight + alligator_weight - lion_weight;
    cache.insert("lion", Weight(lion_inner_weight));
    let lion_weight = cache.weight("lion");

    // Make "cow" and "alligator" stale and remove them.
    for _ in 0..(2 * STALE_PERIOD) {
        cache.get(&"lion");

        // The "whale" is something to evict so the stale counter moves.
        cache.insert("whale", Weight(100 * lion_weight));
        cache.evict(2 * lion_weight);
    }

    // Either "cow" and "alligator" fit in the cache, or just "lion".
    // "lion" will be kept, it had lower frequency but was not stale.
    assert!(cache.get(&"cow").is_none());
    assert!(cache.get(&"alligator").is_none());
    assert_eq!(cache.get(&"lion"), Some(&Weight(lion_inner_weight)));
}
