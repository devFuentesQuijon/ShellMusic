//! Letras de canciones vía lrclib.net (sin API key): búsqueda por artista y
//! título, eligiendo la entrada con letra cuya duración más se acerque a la
//! de la pista cuando se conoce.

use std::time::Duration;

use serde::Deserialize;

use crate::providers::Track;

const SEARCH_URL: &str = "https://lrclib.net/api/search";
const TIMEOUT: Duration = Duration::from_secs(10);
const USER_AGENT: &str = concat!("ShellMusic/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrcEntry {
    duration: Option<f64>,
    plain_lyrics: Option<String>,
}

/// Descarga la letra de una pista; errores de red o "sin letra" llegan como
/// String para mostrarlos en la UI (nunca panic).
pub async fn fetch_lyrics(track: &Track) -> Result<String, String> {
    let query = format!("{} {}", track.artist, track.title);
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(TIMEOUT)
        .build()
        .map_err(|err| err.to_string())?;
    let entries: Vec<LrcEntry> = client
        .get(SEARCH_URL)
        .query(&[("q", query.as_str())])
        .send()
        .await
        .map_err(|err| format!("error de red al buscar la letra: {err}"))?
        .json()
        .await
        .map_err(|err| format!("respuesta de letras inválida: {err}"))?;
    pick_lyrics(&entries, track.duration)
        .ok_or_else(|| format!("sin letra para «{}»", track.title))
}

/// La mejor entrada: con letra no vacía, la de duración más cercana a la de
/// la pista (si se conoce); si no, la primera con letra.
fn pick_lyrics(entries: &[LrcEntry], duration: Option<Duration>) -> Option<String> {
    let mut with_lyrics: Vec<&LrcEntry> = entries
        .iter()
        .filter(|e| e.plain_lyrics.as_ref().is_some_and(|l| !l.trim().is_empty()))
        .collect();
    if let Some(target) = duration {
        with_lyrics.sort_by(|a, b| {
            let da = a
                .duration
                .map_or(f64::MAX, |d| (d - target.as_secs_f64()).abs());
            let db = b
                .duration
                .map_or(f64::MAX, |d| (d - target.as_secs_f64()).abs());
            da.total_cmp(&db)
        });
    }
    with_lyrics.first().and_then(|e| e.plain_lyrics.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(duration: Option<f64>, lyrics: Option<&str>) -> LrcEntry {
        LrcEntry {
            duration,
            plain_lyrics: lyrics.map(str::to_string),
        }
    }

    #[test]
    fn parses_lrclib_response() {
        let json = r#"[{"id":1,"trackName":"Song","artistName":"Artist","duration":200.0,"instrumental":false,"plainLyrics":"la la la"}]"#;
        let entries: Vec<LrcEntry> = serde_json::from_str(json).unwrap();
        assert_eq!(entries[0].duration, Some(200.0));
        assert_eq!(entries[0].plain_lyrics.as_deref(), Some("la la la"));
    }

    #[test]
    fn picks_closest_duration_with_lyrics() {
        let entries = vec![
            entry(Some(500.0), Some("letra live")),
            entry(Some(200.0), Some("letra studio")),
            entry(Some(199.0), None),
        ];
        let picked = pick_lyrics(&entries, Some(Duration::from_secs(210)));
        assert_eq!(picked.as_deref(), Some("letra studio"));
    }

    #[test]
    fn without_duration_picks_first_with_lyrics() {
        let entries = vec![
            entry(None, None),
            entry(None, Some("letra b")),
            entry(None, Some("letra c")),
        ];
        assert_eq!(pick_lyrics(&entries, None).as_deref(), Some("letra b"));
    }

    #[test]
    fn no_lyrics_anywhere_is_none() {
        let entries = vec![entry(Some(100.0), Some("   ")), entry(None, None)];
        assert_eq!(pick_lyrics(&entries, None), None);
        assert_eq!(pick_lyrics(&[], None), None);
    }
}
