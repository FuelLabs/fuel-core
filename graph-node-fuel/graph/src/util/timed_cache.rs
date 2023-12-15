use std::{
    borrow::Borrow,
    cmp::Eq,
    collections::HashMap,
    hash::Hash,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

/// Caching of values for a specified amount of time
#[derive(Debug)]
struct CacheEntry<V> {
    value: Arc<V>,
    expires: Instant,
}

/// A cache that keeps entries live for a fixed amount of time. It is assumed
/// that all that data that could possibly wind up in the cache is very small,
/// and that expired entries are replaced by an updated entry whenever expiry
/// is detected. In other words, the cache does not ever remove entries.
#[derive(Debug)]
pub struct TimedCache<K, V> {
    ttl: Duration,
    entries: RwLock<HashMap<K, CacheEntry<V>>>,
}

impl<K, V> TimedCache<K, V> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Return the entry for `key` if it exists and is not expired yet, and
    /// return `None` otherwise. Note that expired entries stay in the cache
    /// as it is assumed that, after returning `None`, the caller will
    /// immediately overwrite that entry with a call to `set`
    pub fn get<Q: ?Sized>(&self, key: &Q) -> Option<Arc<V>>
    where
        K: Borrow<Q> + Eq + Hash,
        Q: Hash + Eq,
    {
        self.get_at(key, Instant::now())
    }

    fn get_at<Q: ?Sized>(&self, key: &Q, now: Instant) -> Option<Arc<V>>
    where
        K: Borrow<Q> + Eq + Hash,
        Q: Hash + Eq,
    {
        match self.entries.read().unwrap().get(key) {
            Some(CacheEntry { value, expires }) if expires >= &now => Some(value.clone()),
            _ => None,
        }
    }

    /// Associate `key` with `value` in the cache. The `value` will be
    /// valid for `self.ttl` duration
    pub fn set(&self, key: K, value: Arc<V>)
    where
        K: Eq + Hash,
    {
        self.set_at(key, value, Instant::now())
    }

    fn set_at(&self, key: K, value: Arc<V>, now: Instant)
    where
        K: Eq + Hash,
    {
        let entry = CacheEntry {
            value,
            expires: now + self.ttl,
        };
        self.entries.write().unwrap().insert(key, entry);
    }

    pub fn clear(&self) {
        self.entries.write().unwrap().clear();
    }

    pub fn find<F>(&self, pred: F) -> Option<Arc<V>>
    where
        F: Fn(&V) -> bool,
    {
        self.entries
            .read()
            .unwrap()
            .values()
            .find(move |entry| pred(entry.value.as_ref()))
            .map(|entry| entry.value.clone())
    }

    /// Remove an entry from the cache. If there was an entry for `key`,
    /// return the value associated with it and whether the entry is still
    /// live
    pub fn remove<Q: ?Sized>(&self, key: &Q) -> Option<(Arc<V>, bool)>
    where
        K: Borrow<Q> + Eq + Hash,
        Q: Hash + Eq,
    {
        self.entries
            .write()
            .unwrap()
            .remove(key)
            .map(|CacheEntry { value, expires }| (value, expires >= Instant::now()))
    }
}

#[test]
fn cache() {
    const KEY: &str = "one";
    let cache = TimedCache::<String, String>::new(Duration::from_millis(10));
    let now = Instant::now();
    cache.set_at(KEY.to_string(), Arc::new("value".to_string()), now);
    assert!(cache.get_at(KEY, now + Duration::from_millis(5)).is_some());
    assert!(cache.get_at(KEY, now + Duration::from_millis(15)).is_none());
}
