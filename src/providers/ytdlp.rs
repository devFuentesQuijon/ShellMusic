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

/// Resuelve una URL de vídeo o playlist directamente, sin prefijo de
/// búsqueda. Las playlists se recortan a `limit` entradas. `extra_args`
/// añade opciones de extracción (anti-bot) antes de las banderas propias.
pub(super) async fn resolve_url(
    provider: ProviderKind,
    url: &str,
    limit: usize,
    extra_args: &[String],
) -> Result<Vec<Track>> {
    let limit = limit.clamp(1, 50);
    let mut args = vec![url.to_string(), format!("--playlist-end={limit}")];
    args.extend(extra_args.iter().cloned());
    let stdout = run_ytdlp(&args).await?;
    let tracks = parse_lines(&stdout, provider);
    if tracks.is_empty() {
        bail!("la URL no devolvió contenido reproducible");
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
    run_ytdlp_with("yt-dlp", args).await
}

/// El nombre del binario es inyectable para poder probar el reintento con un
/// ejecutable falso, sin tocar el PATH global del proceso.
async fn run_ytdlp_with(binary: &str, args: &[String]) -> Result<String> {
    // Un fallo transitorio (timeout, 502) no debe tumbar la búsqueda:
    // un reintento tras una breve espera suele bastar.
    let mut last_err = String::new();
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        match run_ytdlp_once(binary, args).await {
            Ok(stdout) => return Ok(stdout),
            Err(err) => last_err = err.to_string(),
        }
    }
    bail!("{last_err}")
}

async fn run_ytdlp_once(binary: &str, args: &[String]) -> Result<String> {
    let output = Command::new(binary)
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
            ProviderKind::Soundcloud | ProviderKind::Bandcamp => None,
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

    /// Script falso de yt-dlp: falla `failures` veces (contando en un
    /// archivo) y después imprime JSON de una pista.
    fn fake_ytdlp_script(dir: &std::path::Path, failures: u32) -> std::path::PathBuf {
        let script = dir.join("yt-dlp-fake");
        let counter = dir.join("fake-counter");
        let body = format!(
            "#!/bin/sh\n\
             n=$(cat \"{counter}\" 2>/dev/null || echo 0)\n\
             n=$((n+1)); echo $n > \"{counter}\"\n\
             if [ $n -le {failures} ]; then echo 'error transitorio' >&2; exit 1; fi\n\
             echo '{{\"id\":\"x\",\"title\":\"T\",\"uploader\":\"A\",\"duration\":100,\"webpage_url\":\"https://www.youtube.com/watch?v=x\"}}'\n",
            counter = counter.display(),
            failures = failures,
        );
        std::fs::write(&script, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script, perms).unwrap();
        }
        script
    }

    #[tokio::test]
    async fn retry_recovers_from_transient_failure() {
        let dir = std::env::temp_dir().join(format!(
            "shellmusic-ytdlp-retry-ok-{}-{:016x}",
            std::process::id(),
            crate::state_file::random_suffix()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let binary = fake_ytdlp_script(&dir, 1);
        let out = run_ytdlp_with(binary.to_str().unwrap(), &[])
            .await
            .expect("el reintento recupera el fallo transitorio");
        assert!(out.contains("\"id\":\"x\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn retry_gives_up_after_second_failure() {
        let dir = std::env::temp_dir().join(format!(
            "shellmusic-ytdlp-retry-fail-{}-{:016x}",
            std::process::id(),
            crate::state_file::random_suffix()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let binary = fake_ytdlp_script(&dir, 5);
        let err = run_ytdlp_with(binary.to_str().unwrap(), &[])
            .await
            .expect_err("dos fallos seguidos propagan el error");
        assert!(err.to_string().contains("error transitorio"), "{err:#}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
