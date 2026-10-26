//! Estadísticas locales de escucha: conteo de reproducciones por pista y
//! artistas más oídos, persistidas en `stats.json`.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::providers::{ProviderKind, Track, track_key};

const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsEntry {
    pub provider: ProviderKind,
    pub id: String,
    pub title: String,
    pub artist: String,
    /// Duración y URL se conservan para que la vista pueda reproducir.
    #[serde(default)]
    pub duration_secs: Option<u64>,
    #[serde(default)]
    pub source_url: String,
    pub plays: u64,
    /// Última reproducción (segundos desde epoch).
    pub last_played: u64,
}

impl StatsEntry {
    fn to_track(&self) -> Track {
        Track {
            id: self.id.clone(),
            title: self.title.clone(),
            artist: self.artist.clone(),
            duration: self.duration_secs.map(std::time::Duration::from_secs),
            source_url: self.source_url.clone(),
            provider: self.provider,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StatsFile {
    version: u32,
    entries: Vec<StatsEntry>,
}

/// Artista con sus reproducciones acumuladas (vista derivada).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistCount {
    pub artist: String,
    pub plays: u64,
}

#[derive(Debug, Default)]
pub struct Stats {
    pub entries: Vec<StatsEntry>,
}

impl Stats {
    pub fn path() -> Option<std::path::PathBuf> {
        Some(dirs::data_dir()?.join("shellmusic").join("stats.json"))
    }

    /// Carga las estadísticas; ante archivo corrupto o de versión
    /// incompatible lo aparta a `.bak` y empieza vacío, devolviendo un aviso
    /// para la UI. Un error de lectura distinto de "no existe" se propaga.
    pub fn load(path: &Path) -> std::io::Result<(Self, Option<String>)> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Self::default(), None));
            }
            Err(err) => return Err(err),
        };
        match serde_json::from_str::<StatsFile>(&text) {
            Ok(file) if file.version == VERSION => Ok((
                Self {
                    entries: file.entries,
                },
                None,
            )),
            _ => {
                let backup = path.with_extension("json.bak");
                let _ = fs::rename(path, &backup);
                Ok((
                    Self::default(),
                    Some(format!(
                        "stats.json incompatible o corrupto; se guardó copia en {}",
                        backup.display()
                    )),
                ))
            }
        }
    }

    /// Escritura atómica (temporal único + rename) bajo lock de archivo.
    /// Sin merge multi-instancia: gana la última escritura (como la sesión).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        use crate::state_file::{SaveLock, unique_tmp_path};

        let _lock = SaveLock::acquire(path)?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let file = StatsFile {
            version: VERSION,
            entries: self.entries.clone(),
        };
        let tmp = unique_tmp_path(path);
        fs::write(&tmp, serde_json::to_string_pretty(&file)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Registra una reproducción: incrementa el conteo y la fecha.
    pub fn record(&mut self, track: &Track) {
        let key = track_key(track);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        match self
            .entries
            .iter_mut()
            .find(|e| (e.provider, e.id.as_str()) == (key.0, key.1.as_str()))
        {
            Some(entry) => {
                entry.plays += 1;
                entry.last_played = now;
                // Título/artista pueden mejorar con el tiempo (búsquedas
                // distintas): se refrescan con lo último visto.
                entry.title = track.title.clone();
                entry.artist = track.artist.clone();
                entry.duration_secs = track.duration.map(|d| d.as_secs());
                entry.source_url = track.source_url.clone();
            }
            None => self.entries.push(StatsEntry {
                provider: track.provider,
                id: track.id.clone(),
                title: track.title.clone(),
                artist: track.artist.clone(),
                duration_secs: track.duration.map(|d| d.as_secs()),
                source_url: track.source_url.clone(),
                plays: 1,
                last_played: now,
            }),
        }
    }

    pub fn total_plays(&self) -> u64 {
        self.entries.iter().map(|e| e.plays).sum()
    }

    /// Pistas ordenadas por reproducciones (desempate: más reciente primero).
    pub fn top_tracks(&self) -> Vec<(&StatsEntry, Track)> {
        let mut sorted: Vec<&StatsEntry> = self.entries.iter().collect();
        sorted.sort_by(|a, b| {
            b.plays
                .cmp(&a.plays)
                .then(b.last_played.cmp(&a.last_played))
        });
        sorted.into_iter().map(|e| (e, e.to_track())).collect()
    }

    /// Artistas ordenados por reproducciones acumuladas.
    pub fn top_artists(&self) -> Vec<ArtistCount> {
        let mut counts: Vec<ArtistCount> = Vec::new();
        for entry in &self.entries {
            match counts.iter_mut().find(|c| c.artist == entry.artist) {
                Some(count) => count.plays += entry.plays,
                None => counts.push(ArtistCount {
                    artist: entry.artist.clone(),
                    plays: entry.plays,
                }),
            }
        }
        counts.sort_by(|a, b| b.plays.cmp(&a.plays).then(a.artist.cmp(&b.artist)));
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ProviderKind;

    fn track(id: &str, artist: &str) -> Track {
        Track {
            id: id.to_string(),
            title: format!("título {id}"),
            artist: artist.to_string(),
            duration: None,
            source_url: format!("https://example.com/{id}"),
            provider: ProviderKind::Youtube,
        }
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "shellmusic-stats-test-{}-{}",
            std::process::id(),
            name
        ))
    }

    #[test]
    fn record_counts_and_dedupes_by_key() {
        let mut stats = Stats::default();
        stats.record(&track("a", "artista1"));
        stats.record(&track("b", "artista1"));
        stats.record(&track("a", "artista1"));
        assert_eq!(stats.entries.len(), 2);
        let a = stats.entries.iter().find(|e| e.id == "a").unwrap();
        assert_eq!(a.plays, 2);
        assert_eq!(stats.total_plays(), 3);
    }

    #[test]
    fn top_tracks_and_artists_are_sorted() {
        let mut stats = Stats::default();
        stats.record(&track("a", "x"));
        stats.record(&track("b", "y"));
        stats.record(&track("b", "y"));
        stats.record(&track("c", "y"));
        let top = stats.top_tracks();
        assert_eq!(top[0].0.id, "b");
        let artists = stats.top_artists();
        assert_eq!(artists[0].artist, "y");
        assert_eq!(artists[0].plays, 3);
        assert_eq!(artists[1].artist, "x");
    }

    #[test]
    fn persistence_round_trip() {
        let path = temp_path("roundtrip.json");
        let mut stats = Stats::default();
        stats.record(&track("a", "artista"));
        stats.record(&track("a", "artista"));
        stats.save(&path).unwrap();

        let (loaded, warning) = Stats::load(&path).unwrap();
        assert!(warning.is_none());
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].plays, 2);
        assert!(loaded.entries[0].last_played > 0);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn missing_file_loads_empty_without_warning() {
        let (loaded, warning) = Stats::load(&temp_path("inexistente.json")).unwrap();
        assert!(loaded.entries.is_empty());
        assert!(warning.is_none());
    }

    #[test]
    fn corrupt_file_is_backed_up_and_loads_empty() {
        let path = temp_path("corrupt.json");
        fs::write(&path, "no json").unwrap();
        let (loaded, warning) = Stats::load(&path).unwrap();
        assert!(loaded.entries.is_empty());
        assert!(warning.is_some());
        assert!(path.with_extension("json.bak").exists());
        let _ = fs::remove_file(path.with_extension("json.bak"));
    }

    #[test]
    fn read_error_propagates() {
        let path = temp_path("es-directorio.json");
        fs::create_dir_all(&path).unwrap();
        assert!(Stats::load(&path).is_err());
        let _ = fs::remove_dir(&path);
    }
}
