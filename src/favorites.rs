use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::providers::Track;

const VERSION: u32 = 1;

#[derive(Debug, Default, Serialize, Deserialize)]
struct FavoritesFile {
    version: u32,
    tracks: Vec<Track>,
}

#[derive(Debug, Default)]
pub struct Favorites {
    pub tracks: Vec<Track>,
}

impl Favorites {
    pub fn path() -> Option<PathBuf> {
        Some(dirs::data_dir()?.join("shellmusic").join("favorites.json"))
    }

    /// Carga favoritos; ante archivo corrupto o de versión incompatible lo
    /// aparta a `.bak` y empieza vacío, devolviendo un aviso para la UI.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        let Ok(text) = fs::read_to_string(path) else {
            return (Self::default(), None); // no existe aún
        };
        match serde_json::from_str::<FavoritesFile>(&text) {
            Ok(file) if file.version == VERSION => {
                let mut favorites = Self::default();
                for track in file.tracks {
                    favorites.add(track);
                }
                (favorites, None)
            }
            _ => {
                let backup = path.with_extension("json.bak");
                let _ = fs::rename(path, &backup);
                (
                    Self::default(),
                    Some(format!(
                        "favorites.json incompatible o corrupto; se guardó copia en {}",
                        backup.display()
                    )),
                )
            }
        }
    }

    /// Escritura atómica: archivo temporal + rename.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let file = FavoritesFile {
            version: VERSION,
            tracks: self.tracks.clone(),
        };
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(&file)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn contains(&self, track: &Track) -> bool {
        self.tracks
            .iter()
            .any(|t| t.provider == track.provider && t.id == track.id)
    }

    pub fn add(&mut self, track: Track) -> bool {
        if self.contains(&track) {
            return false;
        }
        self.tracks.push(track);
        true
    }

    /// Devuelve true si pasó a ser favorito, false si se quitó.
    pub fn toggle(&mut self, track: &Track) -> bool {
        if let Some(index) = self
            .tracks
            .iter()
            .position(|t| t.provider == track.provider && t.id == track.id)
        {
            self.tracks.remove(index);
            false
        } else {
            self.tracks.push(track.clone());
            true
        }
    }

    pub fn remove(&mut self, index: usize) -> Option<Track> {
        (index < self.tracks.len()).then(|| self.tracks.remove(index))
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
            duration: Some(std::time::Duration::from_secs(185)),
            source_url: format!("https://example.com/{id}"),
            provider: ProviderKind::Youtube,
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "shellmusic-fav-test-{}-{}",
            std::process::id(),
            name
        ))
    }

    #[test]
    fn toggle_adds_and_removes() {
        let mut favorites = Favorites::default();
        assert!(favorites.toggle(&track("a")));
        assert!(favorites.contains(&track("a")));
        assert!(!favorites.toggle(&track("a")));
        assert!(!favorites.contains(&track("a")));
        assert!(favorites.tracks.is_empty());
    }

    #[test]
    fn add_dedupes() {
        let mut favorites = Favorites::default();
        assert!(favorites.add(track("a")));
        assert!(!favorites.add(track("a")));
        assert_eq!(favorites.tracks.len(), 1);
    }

    #[test]
    fn persistence_round_trip() {
        let path = temp_path("roundtrip.json");
        let mut favorites = Favorites::default();
        favorites.add(track("a"));
        favorites.add(track("b"));
        favorites.save(&path).unwrap();

        let (loaded, warning) = Favorites::load(&path);
        assert!(warning.is_none());
        assert_eq!(loaded.tracks.len(), 2);
        assert!(loaded.contains(&track("a")));
        assert_eq!(loaded.tracks[1].title, "título b");
        assert_eq!(loaded.tracks[0].duration, track("a").duration);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn missing_file_loads_empty_without_warning() {
        let (loaded, warning) = Favorites::load(&temp_path("inexistente.json"));
        assert!(loaded.tracks.is_empty());
        assert!(warning.is_none());
    }

    #[test]
    fn corrupt_file_is_backed_up_and_loads_empty() {
        let path = temp_path("corrupt.json");
        fs::write(&path, "{ esto no es json válido").unwrap();
        let (loaded, warning) = Favorites::load(&path);
        assert!(loaded.tracks.is_empty());
        assert!(warning.is_some());
        assert!(!path.exists());
        assert!(path.with_extension("json.bak").exists());
        let _ = fs::remove_file(path.with_extension("json.bak"));
    }

    #[test]
    fn incompatible_version_is_backed_up() {
        let path = temp_path("version.json");
        fs::write(&path, r#"{"version": 99, "tracks": []}"#).unwrap();
        let (loaded, warning) = Favorites::load(&path);
        assert!(loaded.tracks.is_empty());
        assert!(warning.is_some());
        let _ = fs::remove_file(path.with_extension("json.bak"));
    }
}
