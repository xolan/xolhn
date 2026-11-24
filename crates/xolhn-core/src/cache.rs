use std::collections::HashMap;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::api::{HnApi, HnError};
use crate::model::{
    FeedKind, FeedPage, SearchFilter, SearchResults, SearchSort, StoryThread,
    current_unix_timestamp,
};

type FeedKey = (FeedKind, usize);
type SearchKey = (String, SearchFilter, SearchSort, usize);

/// Configuration controlling TTLs and in-memory capacity for [`CacheStore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheConfig {
    pub feed_ttl_secs: u64,
    pub thread_ttl_secs: u64,
    pub search_ttl_secs: u64,
    pub max_memory_entries: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            feed_ttl_secs: 120,
            thread_ttl_secs: 180,
            search_ttl_secs: 120,
            max_memory_entries: 128,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry<T> {
    pub value: T,
    pub cached_at: i64,
}

#[derive(Debug, Clone)]
struct MemoryEntry<T> {
    entry: CacheEntry<T>,
    seq: u64,
}

/// Thread-safe in-memory and optional on-disk cache for Hacker News feeds, threads, and searches.
#[derive(Debug)]
pub struct CacheStore {
    config: CacheConfig,
    disk_dir: Option<PathBuf>,
    feeds: RwLock<HashMap<FeedKey, MemoryEntry<FeedPage>>>,
    threads: RwLock<HashMap<u64, MemoryEntry<StoryThread>>>,
    searches: RwLock<HashMap<SearchKey, MemoryEntry<SearchResults>>>,
    next_seq: AtomicU64,
}

impl CacheStore {
    /// Creates a new [`CacheStore`] with optional disk persistence directory and configuration.
    pub fn new(disk_dir: Option<PathBuf>, config: CacheConfig) -> Self {
        Self {
            config,
            disk_dir,
            feeds: RwLock::new(HashMap::new()),
            threads: RwLock::new(HashMap::new()),
            searches: RwLock::new(HashMap::new()),
            next_seq: AtomicU64::new(0),
        }
    }

    /// Returns the active cache configuration.
    pub fn config(&self) -> CacheConfig {
        self.config
    }

    /// Looks up a feed page, returning `(page, is_fresh)` from memory or disk.
    pub fn get_feed(&self, feed: FeedKind, page: usize, now_unix: i64) -> Option<(FeedPage, bool)> {
        let key = (feed, page);
        let filename = feed_filename(feed, page);
        self.get_with_disk_fallback(
            &self.feeds,
            &key,
            &filename,
            now_unix,
            self.config.feed_ttl_secs,
        )
    }

    /// Stores a feed page in memory and persists it to disk if configured.
    pub fn put_feed(&self, page_data: &FeedPage, now_unix: i64) {
        let key = (page_data.feed, page_data.page);
        let filename = feed_filename(page_data.feed, page_data.page);
        self.put_with_disk(&self.feeds, key, page_data.clone(), &filename, now_unix);
    }

    /// Looks up a story thread, returning `(thread, is_fresh)` from memory or disk.
    pub fn get_thread(&self, story_id: u64, now_unix: i64) -> Option<(StoryThread, bool)> {
        let filename = thread_filename(story_id);
        self.get_with_disk_fallback(
            &self.threads,
            &story_id,
            &filename,
            now_unix,
            self.config.thread_ttl_secs,
        )
    }

    /// Stores a story thread in memory and persists it to disk if configured.
    pub fn put_thread(&self, thread: &StoryThread, now_unix: i64) {
        let story_id = thread.story.id;
        let filename = thread_filename(story_id);
        self.put_with_disk(&self.threads, story_id, thread.clone(), &filename, now_unix);
    }

    /// Looks up fresh search results in memory; returns `None` when absent or expired.
    pub fn get_search(
        &self,
        query: &str,
        filter: SearchFilter,
        sort: SearchSort,
        page: usize,
        now_unix: i64,
    ) -> Option<SearchResults> {
        let key = (query.trim().to_string(), filter, sort, page);
        let map = read_lock(&self.searches);
        let mem = map.get(&key)?;
        if is_fresh(mem.entry.cached_at, now_unix, self.config.search_ttl_secs) {
            Some(mem.entry.value.clone())
        } else {
            None
        }
    }

    /// Stores search results in memory (searches are never persisted to disk).
    pub fn put_search(&self, results: &SearchResults, now_unix: i64) {
        let key = (
            results.query.trim().to_string(),
            results.filter,
            results.sort,
            results.page,
        );
        let entry = CacheEntry {
            value: results.clone(),
            cached_at: now_unix,
        };
        let seq = self.next_seq();
        let mut map = write_lock(&self.searches);
        insert_memory(&mut map, key, entry, seq, self.config.max_memory_entries);
    }

    /// Clears all in-memory cache maps and deletes cached `.json` files in `disk_dir`.
    pub fn clear(&self) -> Result<(), HnError> {
        write_lock(&self.feeds).clear();
        write_lock(&self.threads).clear();
        write_lock(&self.searches).clear();

        if let Some(dir) = &self.disk_dir {
            match std::fs::read_dir(dir) {
                Ok(entries) => {
                    for entry in entries {
                        let entry = entry.map_err(|err| {
                            HnError::Cache(format!("failed to read cache directory entry: {err}"))
                        })?;
                        let path = entry.path();
                        if path.extension().and_then(|ext| ext.to_str()) == Some("json")
                            && path.is_file()
                        {
                            std::fs::remove_file(&path).map_err(|err| {
                                HnError::Cache(format!(
                                    "failed to remove cache file {}: {err}",
                                    path.display()
                                ))
                            })?;
                        }
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(HnError::Cache(format!(
                        "failed to read cache directory {}: {err}",
                        dir.display()
                    )));
                }
            }
        }

        Ok(())
    }

    fn next_seq(&self) -> u64 {
        self.next_seq.fetch_add(1, Ordering::Relaxed)
    }

    fn get_with_disk_fallback<K, V>(
        &self,
        lock: &RwLock<HashMap<K, MemoryEntry<V>>>,
        key: &K,
        filename: &str,
        now_unix: i64,
        ttl_secs: u64,
    ) -> Option<(V, bool)>
    where
        K: Eq + Hash + Clone,
        V: Clone + DeserializeOwned,
    {
        if let Some(mem) = read_lock(lock).get(key) {
            let fresh = is_fresh(mem.entry.cached_at, now_unix, ttl_secs);
            return Some((mem.entry.value.clone(), fresh));
        }

        let dir = self.disk_dir.as_ref()?;
        let disk_entry: CacheEntry<V> = read_disk_json(dir, filename)?;

        let seq = self.next_seq();
        let mut map = write_lock(lock);
        if let Some(existing) = map.get(key)
            && existing.entry.cached_at >= disk_entry.cached_at
        {
            let fresh = is_fresh(existing.entry.cached_at, now_unix, ttl_secs);
            return Some((existing.entry.value.clone(), fresh));
        }

        let fresh = is_fresh(disk_entry.cached_at, now_unix, ttl_secs);
        let value = disk_entry.value.clone();
        insert_memory(
            &mut map,
            key.clone(),
            disk_entry,
            seq,
            self.config.max_memory_entries,
        );
        Some((value, fresh))
    }

    fn put_with_disk<K, V>(
        &self,
        lock: &RwLock<HashMap<K, MemoryEntry<V>>>,
        key: K,
        value: V,
        filename: &str,
        now_unix: i64,
    ) where
        K: Eq + Hash + Clone,
        V: Serialize,
    {
        let entry = CacheEntry {
            value,
            cached_at: now_unix,
        };
        let seq = self.next_seq();

        if let Some(dir) = &self.disk_dir
            && let Err(err) = write_atomic_json(dir, filename, &entry, seq)
        {
            log::warn!(
                "Failed to persist cache file {}/{filename}: {err}",
                dir.display()
            );
        }

        let mut map = write_lock(lock);
        insert_memory(&mut map, key, entry, seq, self.config.max_memory_entries);
    }
}

/// [`HnApi`] decorator adding fresh caching and stale-on-error fallback via [`CacheStore`].
pub struct CachedHnApi<C: HnApi> {
    inner: Arc<C>,
    cache: Arc<CacheStore>,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl<C: HnApi> std::fmt::Debug for CachedHnApi<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedHnApi")
            .field("cache", &self.cache)
            .finish_non_exhaustive()
    }
}

impl<C: HnApi> CachedHnApi<C> {
    /// Wraps `inner` with the provided [`CacheStore`].
    pub fn new(inner: Arc<C>, cache: Arc<CacheStore>) -> Self {
        Self {
            inner,
            cache,
            clock: Arc::new(current_unix_timestamp),
        }
    }

    #[cfg(test)]
    fn with_clock(
        inner: Arc<C>,
        cache: Arc<CacheStore>,
        clock: Arc<dyn Fn() -> i64 + Send + Sync>,
    ) -> Self {
        Self {
            inner,
            cache,
            clock,
        }
    }

    /// Returns a reference to the underlying [`CacheStore`].
    pub fn cache(&self) -> &Arc<CacheStore> {
        &self.cache
    }

    /// Fetches a feed page, optionally bypassing fresh cache hits while preserving stale fallback.
    pub fn fetch_feed_with_options(
        &self,
        feed: FeedKind,
        page: usize,
        force_refresh: bool,
    ) -> Result<FeedPage, HnError> {
        let context = format!("feed {} page {page}", feed.slug());
        self.fetch_with_stale_fallback(
            force_refresh,
            &context,
            |now| self.cache.get_feed(feed, page, now),
            || self.inner.fetch_feed(feed, page),
            |page_data, now| self.cache.put_feed(page_data, now),
        )
    }

    /// Fetches a story thread, optionally bypassing fresh cache hits while preserving stale fallback.
    pub fn fetch_thread_with_options(
        &self,
        story_id: u64,
        force_refresh: bool,
    ) -> Result<StoryThread, HnError> {
        let context = format!("thread {story_id}");
        self.fetch_with_stale_fallback(
            force_refresh,
            &context,
            |now| self.cache.get_thread(story_id, now),
            || self.inner.fetch_thread(story_id),
            |thread, now| self.cache.put_thread(thread, now),
        )
    }

    /// Searches stories or comments, optionally bypassing fresh cache hits.
    pub fn search_with_options(
        &self,
        query: &str,
        filter: SearchFilter,
        sort: SearchSort,
        page: usize,
        force_refresh: bool,
    ) -> Result<SearchResults, HnError> {
        let normalized_query = query.trim();
        let now = (self.clock)();
        if !force_refresh
            && let Some(cached) = self
                .cache
                .get_search(normalized_query, filter, sort, page, now)
        {
            return Ok(cached);
        }

        let results = self.inner.search(normalized_query, filter, sort, page)?;
        self.cache.put_search(&results, (self.clock)());
        Ok(results)
    }

    fn fetch_with_stale_fallback<T, Get, Fetch, Put>(
        &self,
        force_refresh: bool,
        context: &str,
        get_cached: Get,
        fetch_remote: Fetch,
        put_cached: Put,
    ) -> Result<T, HnError>
    where
        Get: FnOnce(i64) -> Option<(T, bool)>,
        Fetch: FnOnce() -> Result<T, HnError>,
        Put: FnOnce(&T, i64),
    {
        let now = (self.clock)();
        let stale = match (force_refresh, get_cached(now)) {
            (false, Some((value, true))) => return Ok(value),
            (_, cached) => cached.map(|(value, _)| value),
        };

        match fetch_remote() {
            Ok(value) => {
                put_cached(&value, (self.clock)());
                Ok(value)
            }
            Err(err @ (HnError::Http(_) | HnError::RateLimited)) => {
                if let Some(stale_value) = stale {
                    log::warn!("Using stale cached {context} after fetch error: {err:?}");
                    Ok(stale_value)
                } else {
                    Err(err)
                }
            }
            Err(err) => Err(err),
        }
    }
}

impl<C: HnApi> HnApi for CachedHnApi<C> {
    fn fetch_feed(&self, feed: FeedKind, page: usize) -> Result<FeedPage, HnError> {
        self.fetch_feed_with_options(feed, page, false)
    }

    fn fetch_thread(&self, story_id: u64) -> Result<StoryThread, HnError> {
        self.fetch_thread_with_options(story_id, false)
    }

    fn search(
        &self,
        query: &str,
        filter: SearchFilter,
        sort: SearchSort,
        page: usize,
    ) -> Result<SearchResults, HnError> {
        self.search_with_options(query, filter, sort, page, false)
    }
}

fn feed_filename(feed: FeedKind, page: usize) -> String {
    format!("feed_{}_{}.json", feed.slug(), page)
}

fn thread_filename(story_id: u64) -> String {
    format!("thread_{story_id}.json")
}

fn is_fresh(cached_at: i64, now_unix: i64, ttl_secs: u64) -> bool {
    if ttl_secs == 0 {
        return false;
    }
    let elapsed = now_unix.saturating_sub(cached_at);
    match u64::try_from(elapsed) {
        Ok(secs) => secs < ttl_secs,
        Err(_) => true,
    }
}

fn read_lock<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_lock<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn insert_memory<K: Eq + Hash + Clone, V>(
    map: &mut HashMap<K, MemoryEntry<V>>,
    key: K,
    entry: CacheEntry<V>,
    seq: u64,
    max_entries: usize,
) {
    if max_entries == 0 {
        map.clear();
        return;
    }

    map.insert(key, MemoryEntry { entry, seq });
    while map.len() > max_entries {
        let Some(oldest_key) = map
            .iter()
            .min_by_key(|(_, value)| (value.entry.cached_at, value.seq))
            .map(|(k, _)| k.clone())
        else {
            break;
        };
        map.remove(&oldest_key);
    }
}

fn read_disk_json<T: DeserializeOwned>(dir: &Path, filename: &str) -> Option<CacheEntry<T>> {
    let path = dir.join(filename);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => {
            log::warn!("Failed to read cache file {}: {err}", path.display());
            return None;
        }
    };

    match serde_json::from_slice(&bytes) {
        Ok(entry) => Some(entry),
        Err(err) => {
            log::warn!("Failed to decode cache file {}: {err}", path.display());
            None
        }
    }
}

fn write_atomic_json<T: Serialize>(
    dir: &Path,
    filename: &str,
    entry: &CacheEntry<T>,
    seq: u64,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let target = dir.join(filename);
    let tmp_path = dir.join(format!(".{filename}.{}.{seq}.tmp", std::process::id()));
    let bytes = serde_json::to_vec(entry).map_err(std::io::Error::other)?;

    if let Err(err) = std::fs::write(&tmp_path, &bytes) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(err);
    }
    if let Err(err) = std::fs::rename(&tmp_path, &target) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(err);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicI64, AtomicUsize};

    use super::*;
    use crate::model::{Comment, SearchCommentHit, SearchHit, Story};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "xolhn_cache_test_{label}_{}_{id}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create temp test directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn sample_story(id: u64) -> Story {
        Story {
            id,
            title: format!("Story {id}"),
            url: Some(format!("https://example.com/{id}")),
            domain: Some("example.com".to_string()),
            author: "pg".to_string(),
            points: 42,
            num_comments: 1,
            created_at_i: 1_700_000_000,
            text_html: None,
            tags: vec!["story".to_string()],
        }
    }

    fn sample_feed_page(feed: FeedKind, page: usize, fetched_at: i64) -> FeedPage {
        FeedPage {
            feed,
            page,
            nb_pages: 5,
            nb_hits: 100,
            stories: vec![sample_story((page as u64) * 100 + 1)],
            fetched_at,
        }
    }

    fn sample_thread(story_id: u64, fetched_at: i64) -> StoryThread {
        StoryThread {
            story: sample_story(story_id),
            comments: vec![Comment {
                id: story_id * 10,
                parent_id: Some(story_id),
                story_id,
                author: Some("commenter".to_string()),
                text_html: "<p>hello</p>".to_string(),
                created_at_i: 1_700_000_100,
                children: Vec::new(),
            }],
            fetched_at,
        }
    }

    fn sample_search_results(
        query: &str,
        filter: SearchFilter,
        sort: SearchSort,
        page: usize,
    ) -> SearchResults {
        SearchResults {
            query: query.to_string(),
            filter,
            sort,
            page,
            nb_pages: 1,
            nb_hits: 1,
            hits: vec![SearchHit::Comment(SearchCommentHit {
                id: 99,
                story_id: 1,
                story_title: "Story 1".to_string(),
                story_url: None,
                author: "alice".to_string(),
                text_html: "<p>match</p>".to_string(),
                created_at_i: 1_700_000_200,
            })],
        }
    }

    struct MockHnApi {
        feed_calls: AtomicUsize,
        thread_calls: AtomicUsize,
        search_calls: AtomicUsize,
        feed_error: Mutex<Option<HnError>>,
        thread_error: Mutex<Option<HnError>>,
        search_error: Mutex<Option<HnError>>,
        last_search_query: Mutex<Option<String>>,
    }

    impl MockHnApi {
        fn new() -> Self {
            Self {
                feed_calls: AtomicUsize::new(0),
                thread_calls: AtomicUsize::new(0),
                search_calls: AtomicUsize::new(0),
                feed_error: Mutex::new(None),
                thread_error: Mutex::new(None),
                search_error: Mutex::new(None),
                last_search_query: Mutex::new(None),
            }
        }

        fn set_feed_error(&self, err: Option<HnError>) {
            *self.feed_error.lock().expect("lock feed_error") = err;
        }

        fn set_thread_error(&self, err: Option<HnError>) {
            *self.thread_error.lock().expect("lock thread_error") = err;
        }
    }

    impl HnApi for MockHnApi {
        fn fetch_feed(&self, feed: FeedKind, page: usize) -> Result<FeedPage, HnError> {
            let call = self.feed_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(err) = self.feed_error.lock().expect("lock feed_error").clone() {
                return Err(err);
            }
            Ok(sample_feed_page(feed, page, 1_000 + call as i64))
        }

        fn fetch_thread(&self, story_id: u64) -> Result<StoryThread, HnError> {
            let call = self.thread_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(err) = self.thread_error.lock().expect("lock thread_error").clone() {
                return Err(err);
            }
            Ok(sample_thread(story_id, 2_000 + call as i64))
        }

        fn search(
            &self,
            query: &str,
            filter: SearchFilter,
            sort: SearchSort,
            page: usize,
        ) -> Result<SearchResults, HnError> {
            self.search_calls.fetch_add(1, Ordering::SeqCst);
            *self
                .last_search_query
                .lock()
                .expect("lock last_search_query") = Some(query.to_string());
            if let Some(err) = self.search_error.lock().expect("lock search_error").clone() {
                return Err(err);
            }
            Ok(sample_search_results(query, filter, sort, page))
        }
    }

    fn make_clock(initial: i64) -> (Arc<AtomicI64>, Arc<dyn Fn() -> i64 + Send + Sync>) {
        let now = Arc::new(AtomicI64::new(initial));
        let now_clone = Arc::clone(&now);
        let clock = Arc::new(move || now_clone.load(Ordering::SeqCst));
        (now, clock)
    }

    #[test]
    fn cache_hit_skips_second_network_call() {
        let inner = Arc::new(MockHnApi::new());
        let cache = Arc::new(CacheStore::new(None, CacheConfig::default()));
        let (now, clock) = make_clock(1_000);
        let api = CachedHnApi::with_clock(Arc::clone(&inner), Arc::clone(&cache), clock);

        let first_feed = api.fetch_feed(FeedKind::Top, 0).expect("first feed");
        now.store(1_050, Ordering::SeqCst);
        let second_feed = api.fetch_feed(FeedKind::Top, 0).expect("second feed");
        assert_eq!(first_feed, second_feed);
        assert_eq!(inner.feed_calls.load(Ordering::SeqCst), 1);

        let first_thread = api.fetch_thread(123).expect("first thread");
        now.store(1_100, Ordering::SeqCst);
        let second_thread = api.fetch_thread(123).expect("second thread");
        assert_eq!(first_thread, second_thread);
        assert_eq!(inner.thread_calls.load(Ordering::SeqCst), 1);

        let first_search = api
            .search("  rust ", SearchFilter::Stories, SearchSort::Relevance, 0)
            .expect("first search");
        let second_search = api
            .search("rust", SearchFilter::Stories, SearchSort::Relevance, 0)
            .expect("second search");
        assert_eq!(first_search, second_search);
        assert_eq!(inner.search_calls.load(Ordering::SeqCst), 1);

        let refreshed_search = api
            .search_with_options(
                "rust",
                SearchFilter::Stories,
                SearchSort::Relevance,
                0,
                true,
            )
            .expect("force-refreshed search");
        assert_eq!(refreshed_search, first_search);
        assert_eq!(inner.search_calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            inner
                .last_search_query
                .lock()
                .expect("lock last_search_query")
                .as_deref(),
            Some("rust")
        );
    }

    #[test]
    fn force_refresh_bypasses_fresh_cache() {
        let inner = Arc::new(MockHnApi::new());
        let cache = Arc::new(CacheStore::new(None, CacheConfig::default()));
        let (_now, clock) = make_clock(1_000);
        let api = CachedHnApi::with_clock(Arc::clone(&inner), Arc::clone(&cache), clock);

        let first = api
            .fetch_feed_with_options(FeedKind::Ask, 0, false)
            .expect("initial feed");
        let refreshed = api
            .fetch_feed_with_options(FeedKind::Ask, 0, true)
            .expect("refreshed feed");
        assert_ne!(first.fetched_at, refreshed.fetched_at);
        assert_eq!(inner.feed_calls.load(Ordering::SeqCst), 2);

        let first_thread = api
            .fetch_thread_with_options(77, false)
            .expect("initial thread");
        let refreshed_thread = api
            .fetch_thread_with_options(77, true)
            .expect("refreshed thread");
        assert_ne!(first_thread.fetched_at, refreshed_thread.fetched_at);
        assert_eq!(inner.thread_calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn ttl_expiry_triggers_refetch() {
        let inner = Arc::new(MockHnApi::new());
        let config = CacheConfig {
            feed_ttl_secs: 10,
            thread_ttl_secs: 20,
            search_ttl_secs: 15,
            max_memory_entries: 16,
        };
        let cache = Arc::new(CacheStore::new(None, config));
        let (now, clock) = make_clock(5_000);
        let api = CachedHnApi::with_clock(Arc::clone(&inner), Arc::clone(&cache), clock);

        api.fetch_feed(FeedKind::Best, 1).expect("feed 1");
        now.store(5_009, Ordering::SeqCst);
        api.fetch_feed(FeedKind::Best, 1).expect("feed 1 fresh");
        assert_eq!(inner.feed_calls.load(Ordering::SeqCst), 1);

        now.store(5_010, Ordering::SeqCst);
        api.fetch_feed(FeedKind::Best, 1).expect("feed 1 expired");
        assert_eq!(inner.feed_calls.load(Ordering::SeqCst), 2);

        api.fetch_thread(999).expect("thread");
        now.store(5_029, Ordering::SeqCst);
        api.fetch_thread(999).expect("thread fresh");
        assert_eq!(inner.thread_calls.load(Ordering::SeqCst), 1);

        now.store(5_030, Ordering::SeqCst);
        api.fetch_thread(999).expect("thread expired");
        assert_eq!(inner.thread_calls.load(Ordering::SeqCst), 2);

        api.search("gpui", SearchFilter::All, SearchSort::Newest, 0)
            .expect("search");
        now.store(5_044, Ordering::SeqCst);
        api.search("gpui", SearchFilter::All, SearchSort::Newest, 0)
            .expect("search fresh");
        assert_eq!(inner.search_calls.load(Ordering::SeqCst), 1);

        now.store(5_045, Ordering::SeqCst);
        api.search("gpui", SearchFilter::All, SearchSort::Newest, 0)
            .expect("search expired");
        assert_eq!(inner.search_calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn stale_fallback_returns_cached_item_on_http_and_rate_limit_errors() {
        let inner = Arc::new(MockHnApi::new());
        let config = CacheConfig {
            feed_ttl_secs: 5,
            thread_ttl_secs: 5,
            ..CacheConfig::default()
        };
        let cache = Arc::new(CacheStore::new(None, config));
        let (now, clock) = make_clock(10_000);
        let api = CachedHnApi::with_clock(Arc::clone(&inner), Arc::clone(&cache), clock);

        let initial_feed = api.fetch_feed(FeedKind::Show, 0).expect("seed feed");
        let initial_thread = api.fetch_thread(555).expect("seed thread");

        now.store(10_100, Ordering::SeqCst);

        inner.set_feed_error(Some(HnError::Http("offline".to_string())));
        let stale_feed = api.fetch_feed(FeedKind::Show, 0).expect("stale feed http");
        assert_eq!(stale_feed, initial_feed);

        inner.set_feed_error(Some(HnError::RateLimited));
        let stale_feed_rl = api
            .fetch_feed(FeedKind::Show, 0)
            .expect("stale feed rate limited");
        assert_eq!(stale_feed_rl, initial_feed);

        inner.set_thread_error(Some(HnError::RateLimited));
        let stale_thread = api.fetch_thread(555).expect("stale thread rate limited");
        assert_eq!(stale_thread, initial_thread);

        inner.set_thread_error(Some(HnError::NotFound(555)));
        let err = api
            .fetch_thread(555)
            .expect_err("non-transient error must propagate");
        assert!(matches!(err, HnError::NotFound(555)));
    }

    #[test]
    fn disk_persistence_round_trip_and_clear() {
        let temp = TempDir::new("roundtrip");
        let config = CacheConfig {
            feed_ttl_secs: 60,
            thread_ttl_secs: 60,
            search_ttl_secs: 60,
            max_memory_entries: 16,
        };

        let feed_page = sample_feed_page(FeedKind::New, 2, 3_000);
        let thread = sample_thread(888, 3_000);
        let search = sample_search_results("hn", SearchFilter::All, SearchSort::Relevance, 0);

        {
            let store = CacheStore::new(Some(temp.path().to_path_buf()), config);
            store.put_feed(&feed_page, 3_000);
            store.put_thread(&thread, 3_000);
            store.put_search(&search, 3_000);
        }

        assert!(temp.path().join("feed_new_2.json").is_file());
        assert!(temp.path().join("thread_888.json").is_file());

        let reopened = CacheStore::new(Some(temp.path().to_path_buf()), config);
        let (loaded_feed, feed_fresh) = reopened
            .get_feed(FeedKind::New, 2, 3_010)
            .expect("disk feed hit");
        assert!(feed_fresh);
        assert_eq!(loaded_feed, feed_page);

        let (loaded_thread, thread_stale) = reopened
            .get_thread(888, 3_100)
            .expect("disk thread stale hit");
        assert!(!thread_stale);
        assert_eq!(loaded_thread, thread);

        // Searches are memory-only and must not survive across fresh CacheStore instances.
        assert!(
            reopened
                .get_search("hn", SearchFilter::All, SearchSort::Relevance, 0, 3_010)
                .is_none()
        );

        // Removing the disk file proves the disk hit populated memory.
        std::fs::remove_file(temp.path().join("feed_new_2.json")).expect("remove feed file");
        let (from_memory, still_fresh) = reopened
            .get_feed(FeedKind::New, 2, 3_020)
            .expect("memory populated by disk hit");
        assert!(still_fresh);
        assert_eq!(from_memory, feed_page);

        // Re-seed disk and test clear().
        reopened.put_feed(&feed_page, 3_030);
        assert!(temp.path().join("feed_new_2.json").is_file());
        assert!(temp.path().join("thread_888.json").is_file());

        reopened.clear().expect("clear cache");
        assert!(reopened.get_feed(FeedKind::New, 2, 3_035).is_none());
        assert!(reopened.get_thread(888, 3_035).is_none());
        assert!(!temp.path().join("feed_new_2.json").exists());
        assert!(!temp.path().join("thread_888.json").exists());
    }

    #[test]
    fn memory_eviction_removes_oldest_entries() {
        let config = CacheConfig {
            max_memory_entries: 2,
            ..CacheConfig::default()
        };
        let store = CacheStore::new(None, config);

        store.put_thread(&sample_thread(1, 100), 100);
        store.put_thread(&sample_thread(2, 200), 200);
        store.put_thread(&sample_thread(3, 300), 300);

        assert!(store.get_thread(1, 300).is_none());
        assert!(store.get_thread(2, 300).is_some());
        assert!(store.get_thread(3, 300).is_some());
    }
}
