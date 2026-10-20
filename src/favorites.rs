use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::providers::Track;

const VERSION: u32 = 1;

#[derive(Debug, Default, Serialize, Deserialize)]
struct FavoritesFile {
    version: u32,
    tracks: Vec<Track>,
}

/// mtime + tamaño para detectar escrituras externas entre carga y guardado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    modified: Option<SystemTime>,
    len: u64,
}

impl Fingerprint {
    fn capture(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        Some(Self {
            modified: metadata.modified().ok(),
            len: metadata.len(),
        })
    }
}

#[derive(Debug, Default)]
pub struct Favorites {
    pub tracks: Vec<Track>,
    fingerprint: Option<Fingerprint>,
}

impl Favorites {
    pub fn path() -> Option<PathBuf> {
        Some(dirs::data_dir()?.join("shellmusic").join("favorites.json"))
    }

    /// Carga favoritos; ante archivo corrupto o de versión incompatible lo
    /// aparta a `.bak` y empieza vacío, devolviendo un aviso para la UI.
    /// Un error de lectura distinto de "no existe" se propaga: continuar con
    /// estado vacío dejaría que un guardado posterior borrara el archivo.
    pub fn load(path: &Path) -> std::io::Result<(Self, Option<String>)> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Self::default(), None));
            }
            Err(err) => return Err(err),
        };
        let (mut favorites, warning) = match serde_json::from_str::<FavoritesFile>(&text) {
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
        };
        favorites.fingerprint = Fingerprint::capture(path);
        Ok((favorites, warning))
    }

    /// Escritura atómica bajo lock de archivo: la sección comprobar-huella →
    /// fusionar → escribir → renombrar se serializa entre instancias, y el
    /// temporal es único por escritura. Si el archivo cambió en disco desde
    /// la última huella conocida (otra instancia), se fusiona por
    /// (provider, id) en lugar de sobrescribir a ciegas.
    pub fn save(&mut self, path: &Path) -> std::io::Result<()> {
        use crate::state_file::{SaveLock, unique_tmp_path};

        let _lock = SaveLock::acquire(path)?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        if Fingerprint::capture(path) != self.fingerprint
            && let Ok(text) = fs::read_to_string(path)
            && let Ok(file) = serde_json::from_str::<FavoritesFile>(&text)
            && file.version == VERSION
        {
            for track in file.tracks {
                self.add(track);
            }
        }
        let file = FavoritesFile {
            version: VERSION,
            tracks: self.tracks.clone(),
        };
        let tmp = unique_tmp_path(path);
        fs::write(&tmp, serde_json::to_string_pretty(&file)?)?;
        fs::rename(&tmp, path)?;
        self.fingerprint = Fingerprint::capture(path);
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

        let (loaded, warning) = Favorites::load(&path).unwrap();
        assert!(warning.is_none());
        assert_eq!(loaded.tracks.len(), 2);
        assert!(loaded.contains(&track("a")));
        assert_eq!(loaded.tracks[1].title, "título b");
        assert_eq!(loaded.tracks[0].duration, track("a").duration);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn missing_file_loads_empty_without_warning() {
        let (loaded, warning) = Favorites::load(&temp_path("inexistente.json")).unwrap();
        assert!(loaded.tracks.is_empty());
        assert!(warning.is_none());
    }

    #[test]
    fn read_error_propagates() {
        // Un directorio en el lugar del archivo produce un error de lectura
        // distinto de NotFound: debe propagarse, no cargar vacío.
        let path = temp_path("es-directorio.json");
        fs::create_dir_all(&path).unwrap();
        assert!(Favorites::load(&path).is_err());
        let _ = fs::remove_dir(&path);
    }

    #[test]
    fn save_merges_external_changes() {
        let path = temp_path("merge.json");
        let mut favorites = Favorites::default();
        favorites.add(track("local"));
        favorites.save(&path).unwrap();

        // Otra instancia modifica el archivo por detrás.
        let (mut other, _) = Favorites::load(&path).unwrap();
        other.add(track("externa"));
        other.save(&path).unwrap();

        // Esta instancia guarda sin saberlo: unión, no sobrescritura.
        favorites.add(track("local-2"));
        favorites.save(&path).unwrap();

        let (loaded, warning) = Favorites::load(&path).unwrap();
        assert!(warning.is_none());
        assert!(loaded.contains(&track("local")));
        assert!(loaded.contains(&track("externa")));
        assert!(loaded.contains(&track("local-2")));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn corrupt_file_is_backed_up_and_loads_empty() {
        let path = temp_path("corrupt.json");
        fs::write(&path, "{ esto no es json válido").unwrap();
        let (loaded, warning) = Favorites::load(&path).unwrap();
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
        let (loaded, warning) = Favorites::load(&path).unwrap();
        assert!(loaded.tracks.is_empty());
        assert!(warning.is_some());
        let _ = fs::remove_file(path.with_extension("json.bak"));
    }
}
