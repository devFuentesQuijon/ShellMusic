use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::providers::Track;

const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub name: String,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PlaylistsFile {
    version: u32,
    playlists: Vec<Playlist>,
}

#[derive(Debug, Default)]
pub struct Playlists {
    pub items: Vec<Playlist>,
}

impl Playlists {
    pub fn path() -> Option<PathBuf> {
        Some(dirs::data_dir()?.join("shellmusic").join("playlists.json"))
    }

    /// Carga playlists; ante archivo corrupto o de versión incompatible lo
    /// aparta a `.bak` y empieza vacío, devolviendo un aviso para la UI.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        let Ok(text) = fs::read_to_string(path) else {
            return (Self::default(), None); // no existe aún
        };
        match serde_json::from_str::<PlaylistsFile>(&text) {
            Ok(file) if file.version == VERSION => (
                Self {
                    items: file.playlists,
                },
                None,
            ),
            _ => {
                let backup = path.with_extension("json.bak");
                let _ = fs::rename(path, &backup);
                (
                    Self::default(),
                    Some(format!(
                        "playlists.json incompatible o corrupto; se guardó copia en {}",
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
        let file = PlaylistsFile {
            version: VERSION,
            playlists: self.items.clone(),
        };
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(&file)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Crea una playlist con nombre único (tras recortar espacios).
    pub fn create(&mut self, name: &str) -> Result<usize, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("el nombre no puede estar vacío".to_string());
        }
        if self.items.iter().any(|p| p.name == name) {
            return Err(format!("ya existe una playlist llamada «{name}»"));
        }
        self.items.push(Playlist {
            name: name.to_string(),
            tracks: Vec::new(),
        });
        Ok(self.items.len() - 1)
    }

    pub fn delete(&mut self, index: usize) -> Option<Playlist> {
        (index < self.items.len()).then(|| self.items.remove(index))
    }

    /// Añade sin duplicar (por provider+id); devuelve false si ya estaba.
    pub fn add_track(&mut self, index: usize, track: Track) -> bool {
        let Some(playlist) = self.items.get_mut(index) else {
            return false;
        };
        if playlist
            .tracks
            .iter()
            .any(|t| t.provider == track.provider && t.id == track.id)
        {
            return false;
        }
        playlist.tracks.push(track);
        true
    }

    pub fn remove_track(&mut self, playlist: usize, track: usize) -> Option<Track> {
        let playlist = self.items.get_mut(playlist)?;
        (track < playlist.tracks.len()).then(|| playlist.tracks.remove(track))
    }

    pub fn move_track_up(&mut self, playlist: usize, track: usize) -> bool {
        let Some(playlist) = self.items.get_mut(playlist) else {
            return false;
        };
        if track == 0 || track >= playlist.tracks.len() {
            return false;
        }
        playlist.tracks.swap(track, track - 1);
        true
    }

    pub fn move_track_down(&mut self, playlist: usize, track: usize) -> bool {
        let Some(playlist) = self.items.get_mut(playlist) else {
            return false;
        };
        if track + 1 >= playlist.tracks.len() {
            return false;
        }
        playlist.tracks.swap(track, track + 1);
        true
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
            source_url: format!("https://example.com/{id}"),
            provider: ProviderKind::Youtube,
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "shellmusic-pl-test-{}-{}",
            std::process::id(),
            name
        ))
    }

    #[test]
    fn create_rejects_empty_and_duplicate_names() {
        let mut playlists = Playlists::default();
        assert!(playlists.create("  ").is_err());
        assert_eq!(playlists.create("prueba"), Ok(0));
        assert!(playlists.create("prueba").is_err());
        assert!(playlists.create("  prueba  ").is_err());
        assert_eq!(playlists.items.len(), 1);
    }

    #[test]
    fn add_track_dedupes() {
        let mut playlists = Playlists::default();
        let pl = playlists.create("p").unwrap();
        assert!(playlists.add_track(pl, track("a")));
        assert!(!playlists.add_track(pl, track("a")));
        assert_eq!(playlists.items[pl].tracks.len(), 1);
    }

    #[test]
    fn remove_and_reorder_tracks() {
        let mut playlists = Playlists::default();
        let pl = playlists.create("p").unwrap();
        playlists.add_track(pl, track("a"));
        playlists.add_track(pl, track("b"));
        playlists.add_track(pl, track("c"));
        assert!(!playlists.move_track_up(pl, 0));
        assert!(playlists.move_track_up(pl, 1));
        assert_eq!(playlists.items[pl].tracks[0].id, "b");
        assert!(playlists.move_track_down(pl, 0));
        assert_eq!(playlists.items[pl].tracks[0].id, "a");
        assert!(!playlists.move_track_down(pl, 2));
        assert_eq!(playlists.remove_track(pl, 1).unwrap().id, "b");
        assert_eq!(playlists.items[pl].tracks.len(), 2);
    }

    #[test]
    fn delete_playlist() {
        let mut playlists = Playlists::default();
        playlists.create("a").unwrap();
        playlists.create("b").unwrap();
        assert_eq!(playlists.delete(0).unwrap().name, "a");
        assert_eq!(playlists.items.len(), 1);
        assert!(playlists.delete(5).is_none());
    }

    #[test]
    fn persistence_round_trip() {
        let path = temp_path("roundtrip.json");
        let mut playlists = Playlists::default();
        let pl = playlists.create("mix").unwrap();
        playlists.add_track(pl, track("a"));
        playlists.add_track(pl, track("b"));
        playlists.save(&path).unwrap();

        let (loaded, warning) = Playlists::load(&path);
        assert!(warning.is_none());
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(loaded.items[0].name, "mix");
        assert_eq!(loaded.items[0].tracks[1].title, "título b");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn corrupt_file_is_backed_up_and_loads_empty() {
        let path = temp_path("corrupt.json");
        fs::write(&path, "no json").unwrap();
        let (loaded, warning) = Playlists::load(&path);
        assert!(loaded.items.is_empty());
        assert!(warning.is_some());
        assert!(path.with_extension("json.bak").exists());
        let _ = fs::remove_file(path.with_extension("json.bak"));
    }

    #[test]
    fn incompatible_version_is_backed_up() {
        let path = temp_path("version.json");
        fs::write(&path, r#"{"version": 42, "playlists": []}"#).unwrap();
        let (loaded, warning) = Playlists::load(&path);
        assert!(loaded.items.is_empty());
        assert!(warning.is_some());
        let _ = fs::remove_file(path.with_extension("json.bak"));
    }
}
