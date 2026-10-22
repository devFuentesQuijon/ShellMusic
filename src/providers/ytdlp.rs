use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::Value;
use tokio::process::Command;

use super::{ProviderKind, Track};

pub(super) async fn search(
    prefix: &str,
    provider: ProviderKind,
    query: &str,
    limit: usize,
) -> Result<Vec<Track>> {
    let limit = limit.clamp(1, 50);
    let stdout = run_ytdlp(&[format!("{prefix}{limit}:{query}")]).await?;
    let tracks = parse_lines(&stdout, provider);
    if tracks.is_empty() {
        bail!("sin resultados para «{query}»");
    }
    Ok(tracks)
}

/// La "radio" autogenerada de YouTube para un vídeo (playlist RD<id>):
/// pistas similares según el algoritmo de YouTube. Fallback del modo
/// similares de la radio cuando la última pista es de YouTube.
pub(crate) async fn youtube_mix(video_id: &str) -> Result<Vec<Track>> {
    let url = format!("https://www.youtube.com/watch?v={video_id}&list=RD{video_id}");
    let stdout = run_ytdlp(std::slice::from_ref(&url)).await?;
    let tracks = parse_lines(&stdout, ProviderKind::Youtube);
    if tracks.is_empty() {
        bail!("sin mix para «{url}»");
    }
    Ok(tracks)
}

async fn run_ytdlp(args: &[String]) -> Result<String> {
    let output = Command::new("yt-dlp")
        .args(args)
        .args(["--dump-json", "--flat-playlist", "--no-warnings"])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("no se pudo ejecutar yt-dlp")?;

    if !output.status.success() && output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("yt-dlp falló: {}", stderr.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_lines(stdout: &str, provider: ProviderKind) -> Vec<Track> {
    stdout
        .lines()
        .filter_map(|line| parse_entry(line, provider))
        .collect()
}

fn parse_entry(line: &str, provider: ProviderKind) -> Option<Track> {
    let v: Value = serde_json::from_str(line).ok()?;
    let id = v.get("id")?.as_str()?.to_string();
    let title = v
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Sin título")
        .to_string();
    let artist = ["uploader", "channel", "uploader_id", "channel_id"]
        .iter()
        .find_map(|key| v.get(key).and_then(Value::as_str))
        .unwrap_or("Desconocido")
        .to_string();
    let duration = v
        .get("duration")
        .and_then(Value::as_f64)
        // Una duración negativa sería dato inválido; se clama a 0 porque
        // from_secs_f64 panicua con negativos.
        .map(|s| Duration::from_secs_f64(s.max(0.0)));
    let source_url = ["webpage_url", "url"]
        .iter()
        .find_map(|key| v.get(key).and_then(Value::as_str))
        .filter(|url| url.starts_with("http"))
        .map(str::to_string)
        .or_else(|| match provider {
            ProviderKind::Youtube => Some(format!("https://www.youtube.com/watch?v={id}")),
            ProviderKind::Soundcloud => None,
        })?;
    Some(Track {
        id,
        title,
        artist,
        duration,
        source_url,
        provider,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scsearch_entry_uses_webpage_url() {
        let line = r#"{"id":"254112221","title":"One More Time","uploader":"Daft Punk","duration":320.5,"url":"https://api.soundcloud.com/tracks/254112221","webpage_url":"https://soundcloud.com/daftpunk/one-more-time"}"#;
        let track = parse_entry(line, ProviderKind::Soundcloud).expect("entrada scsearch válida");
        assert_eq!(track.provider, ProviderKind::Soundcloud);
        assert_eq!(track.id, "254112221");
        assert_eq!(track.title, "One More Time");
        assert_eq!(track.artist, "Daft Punk");
        assert_eq!(
            track.source_url,
            "https://soundcloud.com/daftpunk/one-more-time"
        );
        assert_eq!(track.duration, Some(Duration::from_secs_f64(320.5)));
    }

    #[test]
    fn parse_entry_with_negative_duration_clamps_to_zero() {
        let line = r#"{"id":"abc","title":"X","uploader":"Y","duration":-3.5,"webpage_url":"https://www.youtube.com/watch?v=abc"}"#;
        let track = parse_entry(line, ProviderKind::Youtube).expect("entrada válida");
        assert_eq!(track.duration, Some(Duration::ZERO));
    }
}
