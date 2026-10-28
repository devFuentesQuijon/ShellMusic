pub mod bandcamp;
pub mod cache;
pub mod soundcloud;
pub mod youtube;
pub(crate) mod ytdlp;

use std::fmt;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderKind {
    Youtube,
    Soundcloud,
    Bandcamp,
}

impl ProviderKind {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "youtube" => Some(Self::Youtube),
            "soundcloud" => Some(Self::Soundcloud),
            "bandcamp" => Some(Self::Bandcamp),
            _ => None,
        }
    }

    /// Nombre máquina estable del proveedor (config, protocolos).
    pub fn name(self) -> &'static str {
        match self {
            Self::Youtube => "youtube",
            Self::Soundcloud => "soundcloud",
            Self::Bandcamp => "bandcamp",
        }
    }
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderKind::Youtube => write!(f, "YouTube"),
            ProviderKind::Soundcloud => write!(f, "SoundCloud"),
            ProviderKind::Bandcamp => write!(f, "Bandcamp"),
        }
    }
}

/// Si la consulta es una URL de un proveedor conocido (YouTube, SoundCloud
/// o Bandcamp), devuelve ese proveedor: la búsqueda la resuelve yt-dlp
/// directamente contra la URL en lugar de buscar el texto literal.
pub fn provider_for_url(query: &str) -> Option<ProviderKind> {
    let url = url::Url::parse(query.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    match host {
        "youtube.com" | "music.youtube.com" | "m.youtube.com" | "youtu.be" => {
            Some(ProviderKind::Youtube)
        }
        "soundcloud.com" | "on.soundcloud.com" => Some(ProviderKind::Soundcloud),
        // Bandcamp sirve cada artista en un subdominio propio.
        "bandcamp.com" => Some(ProviderKind::Bandcamp),
        _ if host.ends_with(".bandcamp.com") => Some(ProviderKind::Bandcamp),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration: Option<Duration>,
    pub source_url: String,
    pub provider: ProviderKind,
}

/// Clave de identidad de una pista para dedupe y merges de estado local.
pub(crate) fn track_key(track: &Track) -> (ProviderKind, String) {
    (track.provider, track.id.clone())
}

#[async_trait]
pub trait MusicProvider: Send + Sync {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_youtube_watch_resolves_youtube() {
        assert_eq!(
            provider_for_url("https://www.youtube.com/watch?v=abc123XYZ_-"),
            Some(ProviderKind::Youtube)
        );
    }

    #[test]
    fn url_youtu_be_music_and_playlist_resolve_youtube() {
        assert_eq!(
            provider_for_url("https://youtu.be/abc123XYZ_-"),
            Some(ProviderKind::Youtube)
        );
        assert_eq!(
            provider_for_url("https://music.youtube.com/watch?v=abc123XYZ_-&list=RDabc123XYZ_-"),
            Some(ProviderKind::Youtube)
        );
        assert_eq!(
            provider_for_url("https://www.youtube.com/playlist?list=PLrAXtmRdnEQy8JkxD4Lzq8fLeiZUwyZv"),
            Some(ProviderKind::Youtube)
        );
    }

    #[test]
    fn url_soundcloud_track_and_share_resolve_soundcloud() {
        assert_eq!(
            provider_for_url("https://soundcloud.com/daftpunk/one-more-time"),
            Some(ProviderKind::Soundcloud)
        );
        assert_eq!(
            provider_for_url("https://on.soundcloud.com/AbCdEfG"),
            Some(ProviderKind::Soundcloud)
        );
    }

    #[test]
    fn url_bandcamp_apex_and_artist_subdomain_resolve_bandcamp() {
        assert_eq!(
            provider_for_url("https://bandcamp.com/discover/electronic"),
            Some(ProviderKind::Bandcamp)
        );
        assert_eq!(
            provider_for_url("https://sufjanstevens.bandcamp.com/album/carrie-lowell"),
            Some(ProviderKind::Bandcamp)
        );
        assert_eq!(
            provider_for_url("https://artist.bandcamp.com/track/song"),
            Some(ProviderKind::Bandcamp)
        );
    }

    #[test]
    fn lookalike_domains_are_not_bandcamp() {
        assert_eq!(provider_for_url("https://notbandcamp.com/track/x"), None);
        assert_eq!(provider_for_url("https://bandcamp.com.evil.example/x"), None);
    }

    #[test]
    fn provider_names_round_trip() {
        for (name, kind) in [
            ("youtube", ProviderKind::Youtube),
            ("soundcloud", ProviderKind::Soundcloud),
            ("bandcamp", ProviderKind::Bandcamp),
        ] {
            assert_eq!(ProviderKind::from_name(name), Some(kind));
        }
        assert_eq!(ProviderKind::from_name("spotify"), None);
    }

    #[test]
    fn plain_text_and_unknown_hosts_are_not_urls() {
        assert_eq!(provider_for_url("daft punk"), None);
        assert_eq!(provider_for_url("https://example.com/watch?v=x"), None);
        assert_eq!(provider_for_url("ftp://youtube.com/watch?v=x"), None);
        assert_eq!(provider_for_url(""), None);
    }
}
