use anyhow::{Result, bail};
use async_trait::async_trait;

use super::ytdlp;
use super::{MusicProvider, ProviderKind, Track, provider_for_url};

/// Catálogo de Bandcamp. yt-dlp no implementa búsqueda de texto para
/// Bandcamp (solo extrae pistas, álbumes y páginas de artista), así que
/// este proveedor resuelve URLs y rechaza la búsqueda literal con una
/// indicación accionable. Búsqueda de texto vía APIs privadas de Bandcamp:
/// descartada (indocumentada, frágil y gris en ToS; ver SPEC.md fase 19).
pub struct BandcampProvider;

impl BandcampProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BandcampProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MusicProvider for BandcampProvider {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>> {
        if provider_for_url(query) == Some(ProviderKind::Bandcamp) {
            return ytdlp::resolve_url(ProviderKind::Bandcamp, query.trim(), limit, &[]).await;
        }
        bail!(
            "Bandcamp no admite búsqueda de texto; pega una URL de bandcamp.com \
             (pista, álbum o página de artista)"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn text_search_explains_url_usage() {
        let err = BandcampProvider::new()
            .search("daft punk", 15)
            .await
            .expect_err("la búsqueda de texto debe indicar que use una URL");
        assert!(err.to_string().contains("bandcamp.com"), "{err:#}");
    }
}
