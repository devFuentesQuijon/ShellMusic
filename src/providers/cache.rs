use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;

use super::{MusicProvider, ProviderKind, Track};

const MAX_ENTRIES: usize = 64;

type CacheKey = (ProviderKind, String, usize);

struct Entry {
    tracks: Vec<Track>,
    inserted: Instant,
}

/// Decorador que cachea búsquedas en memoria durante la sesión.
pub struct CachingProvider {
    inner: Arc<dyn MusicProvider>,
    kind: ProviderKind,
    ttl: Duration,
    entries: Mutex<HashMap<CacheKey, Entry>>,
}

impl CachingProvider {
    pub fn new(inner: Arc<dyn MusicProvider>, kind: ProviderKind, ttl: Duration) -> Self {
        Self {
            inner,
            kind,
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl MusicProvider for CachingProvider {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>> {
        let key = (self.kind, query.trim().to_lowercase(), limit);
        {
            let mut entries = self.entries.lock().unwrap();
            if let Some(entry) = entries.get(&key) {
                if entry.inserted.elapsed() < self.ttl {
                    return Ok(entry.tracks.clone());
                }
                entries.remove(&key);
            }
        }
        let tracks = self.inner.search(query, limit).await?;
        let mut entries = self.entries.lock().unwrap();
        if entries.len() >= MAX_ENTRIES {
            entries.retain(|_, entry| entry.inserted.elapsed() < self.ttl);
            if entries.len() >= MAX_ENTRIES
                && let Some(oldest) = entries
                    .iter()
                    .max_by_key(|(_, entry)| entry.inserted.elapsed())
                    .map(|(key, _)| key.clone())
            {
                entries.remove(&oldest);
            }
        }
        entries.insert(
            key,
            Entry {
                tracks: tracks.clone(),
                inserted: Instant::now(),
            },
        );
        Ok(tracks)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct CountingProvider {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl MusicProvider for CountingProvider {
        async fn search(&self, query: &str, _limit: usize) -> Result<Vec<Track>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![Track {
                id: query.to_string(),
                title: query.to_string(),
                artist: "artista".to_string(),
                duration: None,
                source_url: format!("https://example.com/{query}"),
                provider: ProviderKind::Youtube,
            }])
        }
    }

    fn counting() -> (Arc<CountingProvider>, CachingProvider) {
        let inner = Arc::new(CountingProvider {
            calls: AtomicUsize::new(0),
        });
        let cache = CachingProvider::new(
            inner.clone(),
            ProviderKind::Youtube,
            Duration::from_secs(60),
        );
        (inner, cache)
    }

    #[tokio::test]
    async fn repeated_query_hits_cache() {
        let (inner, cache) = counting();
        cache.search("Daft Punk ", 10).await.unwrap();
        cache.search("daft punk", 10).await.unwrap();
        assert_eq!(inner.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn expired_entry_reinvokes() {
        let inner = Arc::new(CountingProvider {
            calls: AtomicUsize::new(0),
        });
        let cache = CachingProvider::new(inner.clone(), ProviderKind::Youtube, Duration::ZERO);
        cache.search("daft punk", 10).await.unwrap();
        cache.search("daft punk", 10).await.unwrap();
        assert_eq!(inner.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn distinct_keys_do_not_collide() {
        let (inner, cache) = counting();
        cache.search("daft punk", 10).await.unwrap();
        cache.search("daft punk", 20).await.unwrap();
        cache.search("justice", 10).await.unwrap();
        assert_eq!(inner.calls.load(Ordering::SeqCst), 3);
    }
}
