//! Exportación e importación de datos locales: JSON (playlists y favoritos)
//! y M3U (playlists). Funciones puras y testeables; la app solo orquesta.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::playlists::Playlist;
use crate::providers::{ProviderKind, Track};

const VERSION: u32 = 1;

/// Qué incluir en una exportación JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportScope {
    All,
    Playlists,
    Favorites,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BackupFile {
    pub version: u32,
    #[serde(default)]
    pub favorites: Vec<Track>,
    #[serde(default)]
    pub playlists: Vec<Playlist>,
}

/// Serializa la copia de seguridad JSON según el ámbito elegido.
pub fn export_json(scope: ExportScope, favorites: &[Track], playlists: &[Playlist]) -> String {
    let file = BackupFile {
        version: VERSION,
        favorites: if scope != ExportScope::Playlists {
            favorites.to_vec()
        } else {
            Vec::new()
        },
        playlists: if scope != ExportScope::Favorites {
            playlists.to_vec()
        } else {
            Vec::new()
        },
    };
    serde_json::to_string_pretty(&file).unwrap_or_else(|_| "{}".to_string())
}

/// Parsea una copia de seguridad JSON; versiones incompatibles son error.
pub fn import_json(text: &str) -> Result<BackupFile, String> {
    let file: BackupFile =
        serde_json::from_str(text).map_err(|err| format!("JSON de copia inválido: {err}"))?;
    if file.version != VERSION {
        return Err(format!(
            "versión de copia no soportada: {} (esperada {VERSION})",
            file.version
        ));
    }
    Ok(file)
}

/// M3U extendido: EXTINF con duración (si se conoce) y «artista - título».
pub fn export_m3u(playlist: &Playlist) -> String {
    let mut out = String::from("#EXTM3U\n");
    for track in &playlist.tracks {
        let secs = track.duration.map_or(-1, |d| d.as_secs() as i64);
        out.push_str(&format!("#EXTINF:{secs},{} - {}\n", track.artist, track.title));
        out.push_str(&track.source_url);
        out.push('\n');
    }
    out
}

/// Parsea un M3U a pistas. Las URLs de YouTube extraen id y provider; el
/// resto usa la URL como id (dedupe estable) con provider YouTube.
pub fn import_m3u(text: &str) -> Result<Vec<Track>, String> {
    let mut tracks = Vec::new();
    let mut extinf: Option<(Option<u64>, String)> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line == "#EXTM3U" {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let (secs, title) = rest.split_once(',').unwrap_or(("-1", rest));
            let secs = secs.trim().parse::<i64>().ok().filter(|s| *s >= 0).map(|s| s as u64);
            extinf = Some((secs, title.trim().to_string()));
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        tracks.push(track_from_url(line, extinf.take()));
    }
    if tracks.is_empty() {
        return Err("el M3U no contiene ninguna pista".to_string());
    }
    Ok(tracks)
}

fn track_from_url(url: &str, extinf: Option<(Option<u64>, String)>) -> Track {
    let (duration_secs, label) = extinf.unwrap_or((None, String::new()));
    let (artist, title) = match label.split_once(" - ") {
        Some((artist, title)) => (artist.trim().to_string(), title.trim().to_string()),
        None if !label.is_empty() => (String::new(), label),
        None => (String::new(), url.to_string()),
    };
    let id = youtube_id(url).unwrap_or(url).to_string();
    Track {
        id,
        title,
        artist,
        duration: duration_secs.map(std::time::Duration::from_secs),
        source_url: url.to_string(),
        provider: ProviderKind::Youtube,
    }
}

/// Extrae el id de vídeo de URLs `watch?v=…` o `youtu.be/…`.
fn youtube_id(url: &str) -> Option<&str> {
    if let Some((_, rest)) = url.split_once("watch?v=") {
        return Some(rest.split('&').next().unwrap_or(rest));
    }
    if let Some((_, rest)) = url.split_once("youtu.be/") {
        return Some(rest.split(['?', '&']).next().unwrap_or(rest));
    }
    None
}

/// Expande `~` inicial al home del usuario.
pub fn expand_path(input: &str) -> Result<PathBuf, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("la ruta no puede estar vacía".to_string());
    }
    if let Some(rest) = input.strip_prefix("~/") {
        let home = dirs::home_dir().ok_or("no se pudo determinar el directorio home")?;
        return Ok(home.join(rest));
    }
    Ok(PathBuf::from(input))
}

/// Escribe un archivo de exportación de forma atómica (temporal + rename).
pub fn write_export(path: &Path, contents: &str) -> std::io::Result<()> {
    use crate::state_file::{SaveLock, unique_tmp_path};

    let _lock = SaveLock::acquire(path)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = unique_tmp_path(path);
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Nombre de archivo seguro para una playlist exportada a M3U.
pub fn sanitize_filename(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let clean = clean.trim();
    if clean.is_empty() || !clean.chars().any(|c| c.is_alphanumeric()) {
        "playlist".to_string()
    } else {
        clean.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ProviderKind;

    fn track(id: &str) -> Track {
        Track {
            id: id.to_string(),
            title: format!("título {id}"),
            artist: "artista".to_string(),
            duration: Some(std::time::Duration::from_secs(200)),
            source_url: format!("https://www.youtube.com/watch?v={id}"),
            provider: ProviderKind::Youtube,
        }
    }

    #[test]
    fn json_round_trip() {
        let playlists = vec![Playlist {
            name: "mix".to_string(),
            tracks: vec![track("a"), track("b")],
        }];
        let favorites = vec![track("fav")];
        let text = export_json(ExportScope::All, &favorites, &playlists);
        let file = import_json(&text).unwrap();
        assert_eq!(file.favorites.len(), 1);
        assert_eq!(file.playlists[0].tracks.len(), 2);
    }

    #[test]
    fn json_scopes_filter_sections() {
        let playlists = vec![Playlist {
            name: "p".to_string(),
            tracks: vec![track("a")],
        }];
        let favorites = vec![track("f")];
        let only_playlists = import_json(&export_json(ExportScope::Playlists, &favorites, &playlists)).unwrap();
        assert!(only_playlists.favorites.is_empty());
        assert_eq!(only_playlists.playlists.len(), 1);
        let only_favorites = import_json(&export_json(ExportScope::Favorites, &favorites, &playlists)).unwrap();
        assert_eq!(only_favorites.favorites.len(), 1);
        assert!(only_favorites.playlists.is_empty());
    }

    #[test]
    fn json_incompatible_version_is_error() {
        let err = import_json(r#"{"version": 99}"#).unwrap_err();
        assert!(err.contains("versión"), "{err}");
        assert!(import_json("no json").is_err());
    }

    #[test]
    fn m3u_round_trip() {
        let playlist = Playlist {
            name: "mix".to_string(),
            tracks: vec![track("abc123"), track("def456")],
        };
        let text = export_m3u(&playlist);
        assert!(text.starts_with("#EXTM3U"));
        assert!(text.contains("#EXTINF:200,artista - título abc123"));
        let tracks = import_m3u(&text).unwrap();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].id, "abc123");
        assert_eq!(tracks[0].title, "título abc123");
        assert_eq!(tracks[0].artist, "artista");
        assert_eq!(tracks[0].duration, Some(std::time::Duration::from_secs(200)));
    }

    #[test]
    fn m3u_parses_youtu_be_and_generic_urls() {
        let text = "#EXTM3U\n\
                    https://youtu.be/XYZ789?si=algo\n\
                    #EXTINF:-1,Solo título\n\
                    https://example.com/stream.mp3\n";
        let tracks = import_m3u(text).unwrap();
        assert_eq!(tracks[0].id, "XYZ789");
        assert_eq!(tracks[1].id, "https://example.com/stream.mp3");
        assert_eq!(tracks[1].title, "Solo título");
        assert_eq!(tracks[1].duration, None);
    }

    #[test]
    fn m3u_without_tracks_is_error() {
        assert!(import_m3u("#EXTM3U\n# solo comentarios\n").is_err());
    }

    #[test]
    fn expand_path_handles_tilde_and_plain() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_path("~/copia.json").unwrap(), home.join("copia.json"));
        assert_eq!(expand_path("/tmp/copia.json").unwrap(), PathBuf::from("/tmp/copia.json"));
        assert!(expand_path("   ").is_err());
    }

    #[test]
    fn sanitize_filename_strips_unsafe_chars() {
        assert_eq!(sanitize_filename("mi mix (2026)"), "mi mix _2026_");
        assert_eq!(sanitize_filename("  ///  "), "playlist");
    }
}
