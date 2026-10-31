//! Letras de canciones vía lrclib.net (sin API key): búsqueda por artista y
//! título, eligiendo la entrada con letra cuya duración más se acerque a la
//! de la pista cuando se conoce. Las entradas con `syncedLyrics` (formato
//! LRC) se parsean en líneas con instante para el karaoke de la fase 24.

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
    synced_lyrics: Option<String>,
}

/// Letra de una pista: el texto plano siempre disponible y, cuando la entrada
/// la traía, la versión sincronizada (instante en segundos y texto por línea).
#[derive(Debug, PartialEq)]
pub struct Lyrics {
    pub synced: Option<Vec<(f64, String)>>,
    pub plain: String,
}

/// Descarga la letra de una pista; errores de red o "sin letra" llegan como
/// String para mostrarlos en la UI (nunca panic).
pub async fn fetch_lyrics(track: &Track) -> Result<Lyrics, String> {
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
    pick_lyrics(&entries, track.duration).ok_or_else(|| format!("sin letra para «{}»", track.title))
}

/// La mejor entrada: con letra no vacía, la de duración más cercana a la de
/// la pista (si se conoce); si no, la primera con letra. El texto plano cae
/// al `plainLyrics`; si solo hay sincronizada, se deriva de sus líneas.
fn pick_lyrics(entries: &[LrcEntry], duration: Option<Duration>) -> Option<Lyrics> {
    let mut with_lyrics: Vec<&LrcEntry> = entries
        .iter()
        .filter(|e| {
            e.plain_lyrics
                .as_ref()
                .is_some_and(|l| !l.trim().is_empty())
                || e.synced_lyrics
                    .as_ref()
                    .is_some_and(|l| !l.trim().is_empty())
        })
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
    with_lyrics.first().map(|entry| {
        let synced = entry
            .synced_lyrics
            .as_deref()
            .and_then(parse_lrc)
            .filter(|lines| !lines.is_empty());
        let plain = entry
            .plain_lyrics
            .clone()
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| {
                synced.as_ref().map_or(String::new(), |lines| {
                    lines
                        .iter()
                        .map(|(_, text)| text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                })
            });
        Lyrics { synced, plain }
    })
}

/// Parsea letra en formato LRC: líneas `[mm:ss.xx]texto` (admite varias
/// marcas por línea y las ordena por instante). Las etiquetas de metadata
/// (`[ti:…]`, `[ar:…]`, `[length:…]`) se ignoran. None si no hay ninguna
/// línea con marca de tiempo válida.
fn parse_lrc(text: &str) -> Option<Vec<(f64, String)>> {
    let mut lines: Vec<(f64, String)> = Vec::new();
    for raw in text.lines() {
        let mut rest = raw;
        let mut stamps: Vec<f64> = Vec::new();
        while let Some(start) = rest.find('[') {
            let Some(end_rel) = rest[start..].find(']') else {
                break;
            };
            let end = start + end_rel;
            if let Some(stamp) = parse_timestamp(&rest[start + 1..end]) {
                stamps.push(stamp);
            }
            rest = &rest[end + 1..];
        }
        let text = rest.trim();
        if text.is_empty() {
            continue;
        }
        for stamp in stamps {
            lines.push((stamp, text.to_string()));
        }
    }
    if lines.is_empty() {
        return None;
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    Some(lines)
}

/// `[mm:ss.xx]` → segundos. Los centisegundos llevan tantos dígitos como
/// haya (`[00:12.5]` = 12,5 s); las etiquetas no numéricas dan None.
fn parse_timestamp(tag: &str) -> Option<f64> {
    let (minutes, rest) = tag.split_once(':')?;
    let minutes: f64 = minutes.trim().parse().ok()?;
    let (seconds, fraction) = rest.split_once('.').unwrap_or((rest, ""));
    let seconds: f64 = seconds.trim().parse().ok()?;
    let fraction = fraction.trim();
    let millis: f64 = if fraction.is_empty() {
        0.0
    } else {
        let digits: f64 = fraction.parse().ok()?;
        digits / 10f64.powi(fraction.len() as i32)
    };
    Some(minutes * 60.0 + seconds + millis)
}

/// Índice de la línea activa en `pos` segundos: la última cuyo instante es
/// menor o igual que la posición. Antes de la primera línea, None.
pub(crate) fn active_line(lines: &[(f64, String)], pos: f64) -> Option<usize> {
    lines.iter().rposition(|(stamp, _)| *stamp <= pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(duration: Option<f64>, plain: Option<&str>, synced: Option<&str>) -> LrcEntry {
        LrcEntry {
            duration,
            plain_lyrics: plain.map(str::to_string),
            synced_lyrics: synced.map(str::to_string),
        }
    }

    #[test]
    fn parses_lrclib_response() {
        let json = r#"[{"id":1,"trackName":"Song","artistName":"Artist","duration":200.0,"instrumental":false,"plainLyrics":"la la la","syncedLyrics":"[00:10.00]la la la"}]"#;
        let entries: Vec<LrcEntry> = serde_json::from_str(json).unwrap();
        assert_eq!(entries[0].duration, Some(200.0));
        assert_eq!(entries[0].plain_lyrics.as_deref(), Some("la la la"));
        assert_eq!(
            entries[0].synced_lyrics.as_deref(),
            Some("[00:10.00]la la la")
        );
    }

    #[test]
    fn picks_closest_duration_with_lyrics() {
        let entries = vec![
            entry(Some(500.0), Some("letra live"), None),
            entry(Some(200.0), Some("letra studio"), None),
            entry(Some(199.0), None, None),
        ];
        let picked = pick_lyrics(&entries, Some(Duration::from_secs(210))).unwrap();
        assert_eq!(picked.plain, "letra studio");
        assert_eq!(picked.synced, None);
    }

    #[test]
    fn synced_only_entry_derives_plain_from_lines() {
        let entries = vec![entry(
            None,
            None,
            Some("[00:01.00]primera\n[00:05.00]segunda"),
        )];
        let picked = pick_lyrics(&entries, None).unwrap();
        assert_eq!(picked.plain, "primera\nsegunda");
        assert_eq!(
            picked.synced,
            Some(vec![
                (1.0, "primera".to_string()),
                (5.0, "segunda".to_string())
            ])
        );
    }

    #[test]
    fn without_duration_picks_first_with_lyrics() {
        let entries = vec![
            entry(None, None, None),
            entry(None, Some("letra b"), None),
            entry(None, Some("letra c"), None),
        ];
        let picked = pick_lyrics(&entries, None).unwrap();
        assert_eq!(picked.plain, "letra b");
        assert_eq!(picked.synced, None);
    }

    #[test]
    fn no_lyrics_anywhere_is_none() {
        let entries = vec![
            entry(Some(100.0), Some("   "), None),
            entry(None, None, None),
        ];
        assert_eq!(pick_lyrics(&entries, None), None);
        assert_eq!(pick_lyrics(&[], None), None);
    }

    #[test]
    fn parse_lrc_reads_stamps_and_ignores_metadata() {
        let lrc = "[ti:Song]\n[ar:Artist]\n[00:12.50]media\n[00:01.00]primera\n[length:01:00]\n";
        assert_eq!(
            parse_lrc(lrc),
            Some(vec![
                (1.0, "primera".to_string()),
                (12.5, "media".to_string()),
            ])
        );
    }

    #[test]
    fn parse_lrc_expands_multiple_stamps_per_line() {
        let lrc = "[00:01.00][00:05.00]refrán\n";
        assert_eq!(
            parse_lrc(lrc),
            Some(vec![
                (1.0, "refrán".to_string()),
                (5.0, "refrán".to_string()),
            ])
        );
    }

    #[test]
    fn parse_lrc_rejects_stamp_only_or_text_only_lines() {
        assert_eq!(parse_lrc("[00:01.00]\n"), None, "sin texto no es línea");
        assert_eq!(
            parse_lrc("solo texto sin marca\n"),
            None,
            "sin marcas no es sincronizada"
        );
    }

    #[test]
    fn parse_timestamp_handles_fraction_widths() {
        assert_eq!(parse_timestamp("01:05"), Some(65.0));
        assert_eq!(parse_timestamp("01:05.5"), Some(65.5));
        assert_eq!(parse_timestamp("01:05.25"), Some(65.25));
        assert_eq!(parse_timestamp("ti"), None);
        assert_eq!(parse_timestamp("length"), None);
    }

    #[test]
    fn active_line_follows_position() {
        let lines = vec![
            (0.0, "uno".to_string()),
            (5.0, "dos".to_string()),
            (9.5, "tres".to_string()),
        ];
        assert_eq!(active_line(&lines, 0.0), Some(0));
        assert_eq!(active_line(&lines, 4.99), Some(0));
        assert_eq!(active_line(&lines, 5.0), Some(1));
        assert_eq!(active_line(&lines, 120.0), Some(2), "la última aguanta");
        assert_eq!(active_line(&lines, -1.0), None, "antes de la primera");
    }
}
