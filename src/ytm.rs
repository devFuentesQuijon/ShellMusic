//! Puente a YouTube Music con cuenta (RF-68): materializa y ejecuta un bridge
//! Python sobre `ytmusicapi` (un comando por invocación, JSON por líneas,
//! mismo sobre `{"ok", "data"|"error"}` que el socket de control). Opt-in:
//! requiere `pip install ytmusicapi` y headers guardados con
//! `smusic --ytm-auth`.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

use crate::providers::{ProviderKind, Track};

/// Bridge embebido en el binario: se materializa en el data dir en el primer
/// uso para no depender de archivos junto al ejecutable.
pub const BRIDGE: &str = include_str!("ytm_bridge.py");

const TIMEOUT: Duration = Duration::from_secs(60);

pub fn bridge_path(data_dir: &Path) -> PathBuf {
    data_dir.join("ytm_bridge.py")
}

pub fn headers_path(data_dir: &Path) -> PathBuf {
    data_dir.join("ytm_headers.json")
}

/// Escribe el bridge en disco si falta o está desactualizado.
pub fn ensure_bridge(data_dir: &Path) -> Result<PathBuf> {
    let path = bridge_path(data_dir);
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    if current != BRIDGE {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("no se pudo crear {}", dir.display()))?;
        }
        std::fs::write(&path, BRIDGE)
            .with_context(|| format!("no se pudo escribir {}", path.display()))?;
    }
    Ok(path)
}

/// `true` cuando el puente y los headers están listos para usarse.
pub fn ready(data_dir: &Path) -> bool {
    headers_path(data_dir).is_file() && bridge_path(data_dir).is_file()
}

/// Ejecuta un comando del bridge y devuelve su `data`.
pub async fn run(data_dir: &Path, args: &[&str]) -> Result<Value> {
    run_with(None, data_dir, args).await
}

/// Ejecuta un comando del bridge. `SHELLMUSIC_YTM_PYTHON` redirige el
/// intérprete (tests); `bridge` sustituye al script (tests con un puente de
/// mentira que imprime JSON enlatado).
async fn run_with(bridge: Option<&Path>, data_dir: &Path, args: &[&str]) -> Result<Value> {
    let embedded = ensure_bridge(data_dir)?;
    let bridge = bridge.unwrap_or(&embedded);
    let python = std::env::var("SHELLMUSIC_YTM_PYTHON").unwrap_or_else(|_| "python3".into());
    let mut command = tokio::process::Command::new(&python);
    command
        .arg(bridge)
        .arg(headers_path(data_dir))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(TIMEOUT, command.output())
        .await
        .context("el puente de YouTube Music agotó el tiempo de espera")??;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Ok(value) = serde_json::from_str::<Value>(stdout.trim()) {
        // El bridge responde siempre con sobre, también en los errores de API.
        if value.get("ok").and_then(Value::as_bool) == Some(true) {
            return Ok(value
                .get("data")
                .cloned()
                .unwrap_or(Value::Null));
        }
        let error = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("error desconocido del puente");
        return Err(anyhow!(error.to_string()));
    }
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("ModuleNotFoundError") && stderr.contains("ytmusicapi") {
            bail!("falta ytmusicapi: instálalo con «pip install ytmusicapi»");
        }
        bail!(
            "el puente de YouTube Music falló: {}",
            stderr.trim().lines().next().unwrap_or("sin detalle")
        );
    }
    Err(anyhow!("respuesta del puente de YouTube Music inválida"))
}

/// Pista del protocolo del bridge → `Track` (siempre de YouTube).
pub fn parse_track(value: &Value) -> Option<Track> {
    let id = value.get("id").and_then(Value::as_str)?;
    if id.is_empty() {
        return None;
    }
    Some(Track {
        id: id.to_string(),
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        artist: value
            .get("artist")
            .and_then(Value::as_str)
            .unwrap_or("Desconocido")
            .to_string(),
        duration: value
            .get("duration_secs")
            .and_then(Value::as_u64)
            .map(std::time::Duration::from_secs),
        source_url: value
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        provider: ProviderKind::Youtube,
    })
}

pub fn parse_tracks(values: &[Value]) -> Vec<Track> {
    values.iter().filter_map(parse_track).collect()
}

/// Da (o quita) like en YouTube Music sin esperar respuesta: es «lo mejor
/// posible»; un fallo no interrumpe nada y se descarta.
pub fn spawn_like(data_dir: PathBuf, video_id: String, like: bool) {
    tokio::spawn(async move {
        let command: &str = if like { "like" } else { "unlike" };
        let _ = run(&data_dir, &[command, &video_id]).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_track_maps_bridge_json() {
        let value = serde_json::json!({
            "id": "abc123",
            "title": "Canción",
            "artist": "Artista",
            "duration_secs": 214,
            "url": "https://music.youtube.com/watch?v=abc123",
            "provider": "youtube"
        });
        let track = parse_track(&value).unwrap();
        assert_eq!(track.id, "abc123");
        assert_eq!(track.title, "Canción");
        assert_eq!(track.duration, Some(std::time::Duration::from_secs(214)));
        assert_eq!(track.provider, ProviderKind::Youtube);
        // Sin id (entradas no reproducibles) se descarta.
        assert!(parse_track(&serde_json::json!({"title": "x"})).is_none());
        assert!(parse_track(&serde_json::json!({"id": ""})).is_none());
    }

    #[test]
    fn ensure_bridge_writes_and_updates() {
        let dir = std::env::temp_dir().join(format!(
            "shellmusic-ytm-bridge-{}-{:016x}",
            std::process::id(),
            crate::state_file::random_suffix()
        ));
        let path = ensure_bridge(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), BRIDGE);
        // Segunda llamada: no reescribe (contenido igual).
        assert_eq!(ensure_bridge(&dir).unwrap(), path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Bridge de mentira (python) para probar el runner sin red ni auth:
    /// responde según el comando con sobre ok/error.
    #[cfg(unix)]
    #[tokio::test]
    async fn run_handles_ok_and_error_envelopes() {
        let dir = std::env::temp_dir().join(format!(
            "shellmusic-ytm-run-{}-{:016x}",
            std::process::id(),
            crate::state_file::random_suffix()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake_bridge.py");
        let script = r#"#!/usr/bin/env python3
import json, sys
cmd = sys.argv[2]  # argv[1] es el path de headers, que el falso ignora
if cmd == "boom":
    print(json.dumps({"ok": False, "error": "falló la API"}))
elif cmd == "crash":
    sys.stderr.write("ModuleNotFoundError: No module named 'ytmusicapi'\n")
    sys.exit(1)
else:
    print(json.dumps({"ok": True, "data": {"tracks": [
        {"id": "a", "title": "T", "artist": "R", "duration_secs": 1, "url": "", "provider": "youtube"}
    ]}}))
"#;
        std::fs::write(&fake, script).unwrap();

        let data = run_with(Some(&fake), &dir, &["likes"]).await.unwrap();
        let tracks = parse_tracks(data["tracks"].as_array().unwrap());
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, "a");

        let err = run_with(Some(&fake), &dir, &["boom"]).await.unwrap_err();
        assert!(err.to_string().contains("falló la API"), "error: {err}");

        let err = run_with(Some(&fake), &dir, &["crash"]).await.unwrap_err();
        assert!(
            err.to_string().contains("pip install ytmusicapi"),
            "error accionable: {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
