use anyhow::Result;
use async_trait::async_trait;

use super::ytdlp;
use super::{MusicProvider, ProviderKind, Track, provider_for_url};

pub struct SoundcloudProvider;

impl SoundcloudProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SoundcloudProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MusicProvider for SoundcloudProvider {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>> {
        if provider_for_url(query) == Some(ProviderKind::Soundcloud) {
            return ytdlp::resolve_url(ProviderKind::Soundcloud, query.trim(), limit, &[]).await;
        }
        ytdlp::search("scsearch", ProviderKind::Soundcloud, query, limit).await
    }
}
