use anyhow::Result;
use async_trait::async_trait;

use super::ytdlp;
use super::{MusicProvider, ProviderKind, Track};

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
        ytdlp::search("scsearch", ProviderKind::Soundcloud, query, limit).await
    }
}
