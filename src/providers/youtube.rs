use anyhow::Result;
use async_trait::async_trait;

use crate::config::Youtube;

use super::ytdlp;
use super::{MusicProvider, ProviderKind, Track, provider_for_url};

pub struct YoutubeProvider {
    player_client: String,
    remote_components: String,
}

impl YoutubeProvider {
    pub fn new(youtube: &Youtube) -> Self {
        Self {
            player_client: youtube.player_client.trim().to_string(),
            remote_components: youtube.remote_components.trim().to_string(),
        }
    }

    /// Argumentos anti-bot de yt-dlp para la resolución directa de URLs: el
    /// mismo primer intento que hace la reproducción (cliente configurado +
    /// solucionador de retos JS). Sin ellos YouTube responde «sign in to
    /// confirm you're not a bot» a la extracción de la página del vídeo.
    fn anti_bot_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if !self.player_client.is_empty() {
            args.push("--extractor-args".to_string());
            args.push(format!("youtube:player_client={}", self.player_client));
        }
        if !self.remote_components.is_empty() {
            args.push("--remote-components".to_string());
            args.push(self.remote_components.clone());
        }
        args
    }
}

impl Default for YoutubeProvider {
    fn default() -> Self {
        Self::new(&Youtube::default())
    }
}

#[async_trait]
impl MusicProvider for YoutubeProvider {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>> {
        if provider_for_url(query) == Some(ProviderKind::Youtube) {
            return ytdlp::resolve_url(
                ProviderKind::Youtube,
                query.trim(),
                limit,
                &self.anti_bot_args(),
            )
            .await;
        }
        ytdlp::search("ytsearch", ProviderKind::Youtube, query, limit).await
    }
}
