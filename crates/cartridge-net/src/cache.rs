use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// A single cached HTTP response stored on disk.
#[derive(Serialize, Deserialize)]
struct CacheEntry {
    url: String,
    body: String,
    cached_at: u64,
}

/// File-based HTTP response cache.
///
/// Each cached response is stored as `{cache_dir}/{hash}.json` where the hash
/// is derived from the URL.
pub struct DiskCache {
    cache_dir: PathBuf,
}

impl DiskCache {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Look up a cached response for `url`. Returns `Some(body)` if a valid
    /// (non-expired) entry exists, `None` otherwise.
    pub fn get(&self, url: &str, ttl_seconds: u64) -> Option<String> {
        let path = self.entry_path(url);
        let content = fs::read_to_string(&path).ok()?;
        let entry: CacheEntry = serde_json::from_str(&content).ok()?;

        let now = now_unix();
        if now.saturating_sub(entry.cached_at) >= ttl_seconds {
            // Expired — remove stale file.
            fs::remove_file(&path).ok();
            return None;
        }

        Some(entry.body)
    }

    /// Store a response body in the cache for the given URL.
    pub fn put(&self, url: &str, body: &str) {
        if let Err(e) = fs::create_dir_all(&self.cache_dir) {
            log::warn!("Failed to create cache directory: {e}");
            return;
        }

        let entry = CacheEntry {
            url: url.to_string(),
            body: body.to_string(),
            cached_at: now_unix(),
        };

        let path = self.entry_path(url);
        match serde_json::to_string(&entry) {
            Ok(json) => {
                if let Err(e) = fs::write(&path, json) {
                    log::warn!("Failed to write cache file {}: {e}", path.display());
                }
            }
            Err(e) => {
                log::warn!("Failed to serialize cache entry: {e}");
            }
        }
    }

    fn entry_path(&self, url: &str) -> PathBuf {
        let hash = hash_url(url);
        self.cache_dir.join(format!("{hash}.json"))
    }
}

/// Produce a deterministic numeric hash of a URL for use as a cache key.
fn hash_url(url: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    hasher.finish()
}

/// Current time as seconds since the UNIX epoch.
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let dir = std::env::temp_dir().join("cartridge-net-cache-test");
        let _ = fs::remove_dir_all(&dir);
        let cache = DiskCache::new(dir.clone());

        cache.put("https://example.com/api", r#"{"ok":true}"#);
        let body = cache.get("https://example.com/api", 300);
        assert_eq!(body, Some(r#"{"ok":true}"#.to_string()));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn expired_entry_returns_none() {
        let dir = std::env::temp_dir().join("cartridge-net-cache-expire-test");
        let _ = fs::remove_dir_all(&dir);
        let cache = DiskCache::new(dir.clone());

        cache.put("https://example.com/old", "body");
        // TTL of 0 means immediately expired.
        let body = cache.get("https://example.com/old", 0);
        assert!(body.is_none());

        let _ = fs::remove_dir_all(&dir);
    }
}
