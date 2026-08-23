//! Bounded, process-local cache storage for Thingd adapters.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::{Error, Result};

/// Configuration for a [`MemoryCache`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheOptions {
    /// Maximum number of entries retained by the cache.
    pub max_entries: usize,
    /// Maximum total size of keys and values retained by the cache.
    pub max_bytes: usize,
    /// TTL applied when an entry is inserted without an explicit TTL.
    pub default_ttl: Duration,
}

impl Default for CacheOptions {
    fn default() -> Self {
        Self {
            max_entries: 1_024,
            max_bytes: 64 * 1024 * 1024,
            default_ttl: Duration::from_secs(30),
        }
    }
}

/// Counters and bounded-resource state reported by a [`MemoryCache`].
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct CacheStats {
    /// Number of successful reads.
    pub hits: u64,
    /// Number of reads that did not return a live entry.
    pub misses: u64,
    /// Number of insert operations accepted by the cache.
    pub inserts: u64,
    /// Number of explicit removals.
    pub removals: u64,
    /// Number of entries removed after their TTL elapsed.
    pub expirations: u64,
    /// Number of live entries removed to satisfy an LRU bound.
    pub evictions: u64,
    /// Current number of live entries.
    pub current_entries: usize,
    /// Current size of keys and values in bytes.
    pub current_bytes: usize,
    /// Configured maximum number of entries.
    pub max_entries: usize,
    /// Configured maximum number of key and value bytes.
    pub max_bytes: usize,
}

struct Entry {
    value: Vec<u8>,
    expires_at: Instant,
}

struct CacheState {
    entries: HashMap<Vec<u8>, Entry>,
    lru: VecDeque<Vec<u8>>,
    bytes: usize,
    options: CacheOptions,
    stats: CacheStats,
}

trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// A bounded, TTL-aware, process-local LRU cache.
///
/// The cache stores opaque byte values and performs no filesystem I/O. It is
/// intended for disposable application data such as read-through catalog
/// entries, not for durable state. Values are copied on insertion and read.
#[derive(Clone)]
pub struct MemoryCache {
    state: Arc<Mutex<CacheState>>,
    clock: Arc<dyn Clock>,
}

impl MemoryCache {
    /// Create an empty cache with the supplied bounds and default TTL.
    pub fn new(options: CacheOptions) -> Result<Self> {
        Ok(Self::with_clock(options, Arc::new(SystemClock)))
    }

    fn with_clock(options: CacheOptions, clock: Arc<dyn Clock>) -> Self {
        Self {
            state: Arc::new(Mutex::new(CacheState {
                stats: CacheStats {
                    max_entries: options.max_entries,
                    max_bytes: options.max_bytes,
                    ..CacheStats::default()
                },
                entries: HashMap::new(),
                lru: VecDeque::new(),
                bytes: 0,
                options,
            })),
            clock,
        }
    }

    /// Read a live value and refresh its LRU position.
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        validate_key(key)?;
        let now = self.clock.now();
        let mut state = lock_state(&self.state)?;
        let Some(entry) = state.entries.remove(key) else {
            state.stats.misses = state.stats.misses.saturating_add(1);
            return Ok(None);
        };
        if entry.expires_at <= now {
            state.bytes = state.bytes.saturating_sub(entry_size(key, &entry.value));
            state.lru.retain(|candidate| candidate.as_slice() != key);
            state.stats.expirations = state.stats.expirations.saturating_add(1);
            state.stats.misses = state.stats.misses.saturating_add(1);
            refresh_current_stats(&mut state);
            return Ok(None);
        }
        let value = entry.value.clone();
        state.entries.insert(key.to_vec(), entry);
        touch(&mut state.lru, key);
        state.stats.hits = state.stats.hits.saturating_add(1);
        Ok(Some(value))
    }

    /// Insert or replace a value using the configured default TTL.
    pub fn insert(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let ttl = self.options()?.default_ttl;
        self.insert_with_ttl(key, value, ttl)
    }

    /// Insert or replace a value with an explicit TTL.
    pub fn insert_with_ttl(&self, key: &[u8], value: &[u8], ttl: Duration) -> Result<()> {
        validate_key(key)?;
        let key_size = key.len();
        if key_size.saturating_add(value.len()) > self.options()?.max_bytes
            && self.options()?.max_bytes != 0
        {
            return Err(Error::message("ThingDB cache value exceeds max_bytes"));
        }

        let now = self.clock.now();
        let expires_at = now.checked_add(ttl).unwrap_or(now);
        let mut state = lock_state(&self.state)?;
        remove_expired(&mut state, now);

        if state.options.max_entries == 0 || state.options.max_bytes == 0 {
            return Ok(());
        }

        if let Some(previous) = state.entries.remove(key) {
            state.bytes = state.bytes.saturating_sub(entry_size(key, &previous.value));
            state.lru.retain(|candidate| candidate.as_slice() != key);
        }

        let value = value.to_vec();
        state.bytes = state.bytes.saturating_add(entry_size(key, &value));
        state
            .entries
            .insert(key.to_vec(), Entry { value, expires_at });
        state.lru.push_back(key.to_vec());
        state.stats.inserts = state.stats.inserts.saturating_add(1);

        while state.entries.len() > state.options.max_entries
            || state.bytes > state.options.max_bytes
        {
            let Some(oldest) = state.lru.pop_front() else {
                break;
            };
            if let Some(entry) = state.entries.remove(&oldest) {
                state.bytes = state
                    .bytes
                    .saturating_sub(entry_size(&oldest, &entry.value));
                state.stats.evictions = state.stats.evictions.saturating_add(1);
            }
        }
        refresh_current_stats(&mut state);
        Ok(())
    }

    /// Remove a value, returning whether a live entry was removed.
    pub fn remove(&self, key: &[u8]) -> Result<bool> {
        validate_key(key)?;
        let mut state = lock_state(&self.state)?;
        let Some(entry) = state.entries.remove(key) else {
            return Ok(false);
        };
        state.bytes = state.bytes.saturating_sub(entry_size(key, &entry.value));
        state.lru.retain(|candidate| candidate.as_slice() != key);
        state.stats.removals = state.stats.removals.saturating_add(1);
        refresh_current_stats(&mut state);
        Ok(true)
    }

    /// Remove every entry from the cache.
    pub fn clear(&self) -> Result<()> {
        let mut state = lock_state(&self.state)?;
        state.entries.clear();
        state.lru.clear();
        state.bytes = 0;
        refresh_current_stats(&mut state);
        Ok(())
    }

    /// Return current cache counters and resource usage.
    pub fn stats(&self) -> Result<CacheStats> {
        let mut state = lock_state(&self.state)?;
        remove_expired(&mut state, self.clock.now());
        refresh_current_stats(&mut state);
        Ok(state.stats.clone())
    }

    fn options(&self) -> Result<CacheOptions> {
        Ok(lock_state(&self.state)?.options)
    }
}

fn validate_key(key: &[u8]) -> Result<()> {
    if key.is_empty() {
        return Err(Error::message("ThingDB cache keys must be non-empty"));
    }
    Ok(())
}

fn lock_state(state: &Mutex<CacheState>) -> Result<std::sync::MutexGuard<'_, CacheState>> {
    state
        .lock()
        .map_err(|_| Error::message("ThingDB cache state lock poisoned"))
}

fn entry_size(key: &[u8], value: &[u8]) -> usize {
    key.len().saturating_add(value.len())
}

fn touch(lru: &mut VecDeque<Vec<u8>>, key: &[u8]) {
    lru.retain(|candidate| candidate.as_slice() != key);
    lru.push_back(key.to_vec());
}

fn remove_expired(state: &mut CacheState, now: Instant) {
    let expired = state
        .entries
        .iter()
        .filter(|(_, entry)| entry.expires_at <= now)
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    for key in expired {
        if let Some(entry) = state.entries.remove(&key) {
            state.bytes = state.bytes.saturating_sub(entry_size(&key, &entry.value));
            state.stats.expirations = state.stats.expirations.saturating_add(1);
        }
        state.lru.retain(|candidate| candidate != &key);
    }
    refresh_current_stats(state);
}

fn refresh_current_stats(state: &mut CacheState) {
    state.stats.current_entries = state.entries.len();
    state.stats.current_bytes = state.bytes;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestClock {
        now_ns: AtomicU64,
    }

    impl TestClock {
        fn new() -> Self {
            Self {
                now_ns: AtomicU64::new(0),
            }
        }

        fn advance(&self, duration: Duration) {
            self.now_ns
                .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
        }
    }

    impl Clock for TestClock {
        fn now(&self) -> Instant {
            Instant::now() + Duration::from_nanos(self.now_ns.load(Ordering::Relaxed))
        }
    }

    fn cache(clock: Arc<TestClock>, options: CacheOptions) -> MemoryCache {
        MemoryCache::with_clock(options, clock)
    }

    #[test]
    fn stores_replaces_and_removes_values() {
        let clock = Arc::new(TestClock::new());
        let cache = cache(clock, CacheOptions::default());
        cache.insert(b"key", b"one").unwrap();
        assert_eq!(cache.get(b"key").unwrap(), Some(b"one".to_vec()));
        cache.insert(b"key", b"two").unwrap();
        assert_eq!(cache.get(b"key").unwrap(), Some(b"two".to_vec()));
        assert!(cache.remove(b"key").unwrap());
        assert_eq!(cache.get(b"key").unwrap(), None);
    }

    #[test]
    fn expires_entries_without_sleeping() {
        let clock = Arc::new(TestClock::new());
        let cache = cache(
            clock.clone(),
            CacheOptions {
                default_ttl: Duration::from_secs(10),
                ..CacheOptions::default()
            },
        );
        cache.insert(b"key", b"value").unwrap();
        clock.advance(Duration::from_secs(11));
        assert_eq!(cache.get(b"key").unwrap(), None);
        assert_eq!(cache.stats().unwrap().expirations, 1);
    }

    #[test]
    fn evicts_least_recently_used_entry_by_count() {
        let clock = Arc::new(TestClock::new());
        let cache = cache(
            clock,
            CacheOptions {
                max_entries: 2,
                ..CacheOptions::default()
            },
        );
        cache.insert(b"one", b"1").unwrap();
        cache.insert(b"two", b"2").unwrap();
        assert_eq!(cache.get(b"one").unwrap(), Some(b"1".to_vec()));
        cache.insert(b"three", b"3").unwrap();
        assert_eq!(cache.get(b"two").unwrap(), None);
        assert_eq!(cache.get(b"one").unwrap(), Some(b"1".to_vec()));
        assert_eq!(cache.stats().unwrap().evictions, 1);
    }

    #[test]
    fn enforces_byte_bound_and_rejects_oversized_values() {
        let clock = Arc::new(TestClock::new());
        let cache = cache(
            clock,
            CacheOptions {
                max_bytes: 5,
                ..CacheOptions::default()
            },
        );
        cache.insert(b"a", b"1234").unwrap();
        assert!(cache.insert(b"long", b"value").is_err());
        assert_eq!(cache.stats().unwrap().current_entries, 1);
    }

    #[test]
    fn evicts_oldest_entries_to_satisfy_byte_bound() {
        let clock = Arc::new(TestClock::new());
        let cache = cache(
            clock,
            CacheOptions {
                max_entries: 10,
                max_bytes: 6,
                ..CacheOptions::default()
            },
        );
        cache.insert(b"one", b"1").unwrap();
        cache.insert(b"two", b"2").unwrap();
        assert_eq!(cache.get(b"one").unwrap(), None);
        assert_eq!(cache.get(b"two").unwrap(), Some(b"2".to_vec()));
        assert_eq!(cache.stats().unwrap().evictions, 1);
    }

    #[test]
    fn explicit_ttl_and_clear_update_state() {
        let clock = Arc::new(TestClock::new());
        let cache = cache(clock.clone(), CacheOptions::default());
        cache
            .insert_with_ttl(b"short", b"value", Duration::from_secs(1))
            .unwrap();
        clock.advance(Duration::from_secs(2));
        assert_eq!(cache.get(b"short").unwrap(), None);
        cache.insert(b"long", b"value").unwrap();
        cache.clear().unwrap();
        let stats = cache.stats().unwrap();
        assert_eq!(stats.current_entries, 0);
        assert_eq!(stats.current_bytes, 0);
    }

    #[test]
    fn rejects_empty_keys_and_zero_capacity_is_a_noop() {
        let clock = Arc::new(TestClock::new());
        let cache = cache(
            clock,
            CacheOptions {
                max_entries: 0,
                ..CacheOptions::default()
            },
        );
        assert!(cache.insert(b"", b"value").is_err());
        cache.insert(b"key", b"value").unwrap();
        assert_eq!(cache.get(b"key").unwrap(), None);
    }

    #[test]
    fn independent_instances_do_not_share_state() {
        let first = cache(Arc::new(TestClock::new()), CacheOptions::default());
        let second = cache(Arc::new(TestClock::new()), CacheOptions::default());
        first.insert(b"key", b"value").unwrap();
        assert_eq!(second.get(b"key").unwrap(), None);
    }

    #[test]
    fn concurrent_operations_are_safe() {
        let cache = Arc::new(cache(
            Arc::new(TestClock::new()),
            CacheOptions {
                max_entries: 1_024,
                ..CacheOptions::default()
            },
        ));
        let threads = (0..8)
            .map(|thread| {
                let cache = Arc::clone(&cache);
                std::thread::spawn(move || {
                    for index in 0..128 {
                        let key = format!("{thread}-{index}");
                        cache.insert(key.as_bytes(), b"value").unwrap();
                        assert_eq!(cache.get(key.as_bytes()).unwrap(), Some(b"value".to_vec()));
                    }
                })
            })
            .collect::<Vec<_>>();
        for thread in threads {
            thread.join().unwrap();
        }
        assert!(cache.stats().unwrap().current_entries <= 1_024);
    }
}
