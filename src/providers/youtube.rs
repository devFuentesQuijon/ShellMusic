use anyhow::Result;
use async_trait::async_trait;

use super::ytdlp;
use super::{MusicProvider, ProviderKind, Track};

pub struct YoutubeProvider;

impl YoutubeProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for YoutubeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MusicProvider for YoutubeProvider {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>> {
        ytdlp::search("ytsearch", ProviderKind::Youtube, query, limit).await
    }
}
