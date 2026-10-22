// Copyright 2026 AsterSQL.
//! TTL cache oracle: ttl.go at 42a0da684cf28f16a48c74c54575b711031f3c5d.

pub use astersql_pd_typeutil::Duration as TtlDuration;
use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{runtime::Handle, task::JoinHandle, time::Instant};
use tokio_util::sync::CancellationToken;

/// TTL cache keyed by unsigned IDs, with typed values (`Arc<T>` preserves identity).
pub type TTLUint64<V> = TtlCache<u64, V>;
/// TTL cache keyed by strings, with typed values.
pub type TTLString<V> = TtlCache<String, V>;

struct Item<V> {
    value: V,
    expire: Instant,
}

struct State<K, V> {
    items: HashMap<K, Item<V>>,
    ttl: TtlDuration,
    gc: Option<JoinHandle<()>>,
}

impl<K, V> Drop for State<K, V> {
    fn drop(&mut self) {
        if let Some(task) = &self.gc {
            task.abort();
        }
    }
}

/// Thread-safe TTL storage using the host runtime and its monotonic clock.
/// Expired entries count toward len until removed or collected. Enumeration and
/// pop order are unspecified, matching Go maps. A worker starts on the first put,
/// stops when empty, and restarts on a later put. Cancellation only stops GC.
/// Call shutdown to cancel and join; drop also cancels and aborts any worker.
pub struct TtlCache<K, V> {
    state: Arc<Mutex<State<K, V>>>,
    runtime: Handle,
    cancellation: CancellationToken,
    gc_interval: Duration,
}

impl<K, V> TtlCache<K, V>
where
    K: Clone + Eq + Hash + Send + 'static,
    V: Clone + Send + 'static,
{
    /// Creates an empty cache without starting a worker. Interval must be positive.
    /// TTL is signed Go nanoseconds, including zero and already-expired negative TTLs.
    /// The child cancellation token never cancels the supplied parent token.
    pub fn new(
        runtime: Handle,
        cancellation: CancellationToken,
        gc_interval: Duration,
        ttl: TtlDuration,
    ) -> Self {
        assert!(!gc_interval.is_zero(), "GC interval must be positive");
        Self {
            state: Arc::new(Mutex::new(State {
                items: HashMap::new(),
                ttl,
                gc: None,
            })),
            runtime,
            cancellation: cancellation.child_token(),
            gc_interval,
        }
    }

    /// Inserts using the current default TTL, atomically with UpdateTTL.
    pub fn put(&self, key: K, value: V) {
        let mut state = self.state.lock().unwrap();
        let ttl = state.ttl;
        self.insert(&mut state, key, value, ttl);
    }

    /// Inserts or replaces a key using a per-item TTL.
    pub fn put_with_ttl(&self, key: K, value: V, ttl: TtlDuration) {
        self.insert(&mut self.state.lock().unwrap(), key, value, ttl);
    }

    fn insert(&self, state: &mut State<K, V>, key: K, value: V, ttl: TtlDuration) {
        state.items.insert(
            key,
            Item {
                value,
                expire: expiration(ttl),
            },
        );
        if state.gc.is_some() || self.cancellation.is_cancelled() {
            return;
        }
        let weak = Arc::downgrade(&self.state);
        let cancellation = self.cancellation.clone();
        let interval = self.gc_interval;
        let first_tick = Instant::now() + interval;
        state.gc = Some(self.runtime.spawn(async move {
            let mut ticker = tokio::time::interval_at(first_tick, interval);
            loop {
                tokio::select! {
                    biased;
                    _ = cancellation.cancelled() => return,
                    _ = ticker.tick() => {
                        let Some(shared) = weak.upgrade() else { return };
                        let mut state = shared.lock().unwrap();
                        let now = Instant::now();
                        state.items.retain(|_, item| item.expire >= now);
                        if state.items.is_empty() {
                            // This transition shares the insertion lock, preventing
                            // a put from missing the restart while this worker exits.
                            state.gc = None;
                            return;
                        }
                    }
                }
            }
        }));
    }

    /// Returns a value snapshot, including at the exact expiration instant.
    pub fn get(&self, key: &K) -> Option<V> {
        let state = self.state.lock().unwrap();
        let item = state.items.get(key)?;
        (item.expire >= Instant::now()).then(|| item.value.clone())
    }

    /// Checks validity with the same boundary as get.
    pub fn exists(&self, key: &K) -> bool {
        let state = self.state.lock().unwrap();
        state
            .items
            .get(key)
            .is_some_and(|item| item.expire >= Instant::now())
    }

    /// Returns keys strictly before their expiration, in unspecified order.
    pub fn get_all_id(&self) -> Vec<K> {
        let state = self.state.lock().unwrap();
        let now = Instant::now();
        state
            .items
            .iter()
            .filter(|(_, item)| item.expire > now)
            .map(|(key, _)| key.clone())
            .collect()
    }

    /// Removes and returns an arbitrary entry strictly before expiration.
    pub fn pop(&self) -> Option<(K, V)> {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        let key = state
            .items
            .iter()
            .find(|(_, item)| item.expire > now)?
            .0
            .clone();
        state
            .items
            .remove_entry(&key)
            .map(|(key, item)| (key, item.value))
    }

    /// Removes a key whether valid or expired; absent keys are ignored.
    pub fn remove(&self, key: &K) {
        self.state.lock().unwrap().items.remove(key);
    }

    /// Counts stored entries, including expired entries awaiting GC.
    pub fn len(&self) -> usize {
        self.state.lock().unwrap().items.len()
    }

    /// Reports whether storage is empty, including expired entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes all stored entries. The worker exits on its next tick.
    pub fn clear(&self) {
        self.state.lock().unwrap().items.clear();
    }

    /// Changes the default and refreshes every stored entry, including expired
    /// ones. An unchanged default is a no-op even for per-item TTLs.
    pub fn update_ttl(&self, ttl: TtlDuration) {
        let mut state = self.state.lock().unwrap();
        if state.ttl == ttl {
            return;
        }
        for item in state.items.values_mut() {
            item.expire = expiration(ttl);
        }
        state.ttl = ttl;
    }

    /// Cancels GC and joins its worker; cache entries remain available.
    pub async fn shutdown(&self) {
        self.cancellation.cancel();
        let task = self.state.lock().unwrap().gc.take();
        if let Some(task) = task {
            // Worker panics (including lock poisoning) are not silently ignored.
            task.await.expect("TTL GC worker failed");
        }
    }
}

impl<K, V> Drop for TtlCache<K, V> {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

fn expiration(ttl: TtlDuration) -> Instant {
    let now = Instant::now();
    let nanos = ttl.as_nanos();
    let duration = Duration::from_nanos(nanos.unsigned_abs());
    if nanos < 0 {
        now - duration
    } else {
        now + duration
    }
}
