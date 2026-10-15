use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;

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
        let limit = limit.clamp(1, 50);
        let output = Command::new("yt-dlp")
            .arg(format!("ytsearch{limit}:{query}"))
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

        let stdout = String::from_utf8_lossy(&output.stdout);
        let tracks = stdout.lines().filter_map(parse_entry).collect::<Vec<_>>();
        if tracks.is_empty() {
            bail!("sin resultados para «{query}»");
        }
        Ok(tracks)
    }
}

fn parse_entry(line: &str) -> Option<Track> {
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
        .map(Duration::from_secs_f64);
    let source_url = ["webpage_url", "url"]
        .iter()
        .find_map(|key| v.get(key).and_then(Value::as_str))
        .filter(|url| url.starts_with("http"))
        .map(str::to_string)
        .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={id}"));
    Some(Track {
        id,
        title,
        artist,
        duration,
        source_url,
        provider: ProviderKind::Youtube,
    })
}
