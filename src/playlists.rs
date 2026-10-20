use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::providers::{Track, track_key};

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

/// Contenido del archivo para detectar escrituras externas entre carga y
/// guardado. Se compara contenido, no mtime: hay sistemas de archivos cuya
/// granularidad de mtime no distingue dos escrituras rápidas.
type Fingerprint = Option<String>;

fn capture_fingerprint(path: &Path) -> Fingerprint {
    fs::read_to_string(path).ok()
}

#[derive(Debug, Default)]
pub struct Playlists {
    pub items: Vec<Playlist>,
    fingerprint: Fingerprint,
    /// Snapshot del estado al cargar/guardar: base del merge a 3 bandas.
    base: Vec<Playlist>,
}

impl Playlists {
    pub fn path() -> Option<PathBuf> {
        Some(dirs::data_dir()?.join("shellmusic").join("playlists.json"))
    }

    /// Carga playlists; ante archivo corrupto o de versión incompatible lo
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
        let (mut playlists, warning) = match serde_json::from_str::<PlaylistsFile>(&text) {
            Ok(file) if file.version == VERSION => {
                let base = file.playlists.clone();
                (
                    Self {
                        items: file.playlists,
                        fingerprint: None,
                        base,
                    },
                    None,
                )
            }
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
        };
        playlists.fingerprint = capture_fingerprint(path);
        Ok((playlists, warning))
    }

    /// Escritura atómica bajo lock de archivo: la sección comprobar-huella →
    /// fusionar → escribir → renombrar se serializa entre instancias, y el
    /// temporal es único por escritura. Si el archivo cambió en disco desde
    /// la última huella conocida (otra instancia), se hace merge a 3 bandas
    /// contra la base, a nivel de playlist (por nombre) y de pista dentro de
    /// cada playlist: las adiciones de cualquier lado se conservan y las
    /// eliminaciones de cualquier lado se respetan.
    pub fn save(&mut self, path: &Path) -> std::io::Result<()> {
        use crate::state_file::{SaveLock, unique_tmp_path};

        let _lock = SaveLock::acquire(path)?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        if capture_fingerprint(path) != self.fingerprint
            && let Ok(text) = fs::read_to_string(path)
            && let Ok(file) = serde_json::from_str::<PlaylistsFile>(&text)
            && file.version == VERSION
        {
            self.merge_external(file.playlists);
        }
        let file = PlaylistsFile {
            version: VERSION,
            playlists: self.items.clone(),
        };
        let tmp = unique_tmp_path(path);
        fs::write(&tmp, serde_json::to_string_pretty(&file)?)?;
        fs::rename(&tmp, path)?;
        self.fingerprint = capture_fingerprint(path);
        self.base = self.items.clone();
        Ok(())
    }

    /// Merge a 3 bandas del estado en disco contra la base y la memoria.
    /// A nivel de playlist: lo borrado en cualquier lado queda borrado y lo
    /// añadido en cualquier lado se conserva. Dentro de cada playlist
    /// presente en los tres estados, mismo criterio por pista. Una playlist
    /// nueva con el mismo nombre en ambos lados: gana la de memoria.
    fn merge_external(&mut self, disk: Vec<Playlist>) {
        let base_names: HashSet<&str> = self.base.iter().map(|p| p.name.as_str()).collect();
        let disk_names: HashSet<&str> = disk.iter().map(|p| p.name.as_str()).collect();
        // En base y en memoria pero ya no en disco → eliminada externamente.
        // Ausente de base → añadida localmente: se queda.
        self.items
            .retain(|p| !base_names.contains(p.name.as_str()) || disk_names.contains(p.name.as_str()));

        for disk_playlist in disk {
            match self.items.iter_mut().find(|p| p.name == disk_playlist.name) {
                Some(local) => {
                    // Presente en ambos lados: fusionar pistas si hay base
                    // común; si es nueva en ambos lados, gana la memoria.
                    if let Some(base_playlist) =
                        self.base.iter().find(|p| p.name == disk_playlist.name)
                    {
                        merge_tracks(&base_playlist.tracks, &mut local.tracks, &disk_playlist.tracks);
                    }
                }
                None if !base_names.contains(disk_playlist.name.as_str()) => {
                    // En disco, ni en memoria ni en base → añadida externamente.
                    // (En base y en disco pero no en memoria → borrada localmente.)
                    self.items.push(disk_playlist);
                }
                None => {}
            }
        }
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

/// Merge a 3 bandas de pistas dentro de una playlist: lo eliminado de base
/// por cualquiera de los dos lados queda eliminado; lo añadido por cualquiera
/// de los dos lados se conserva.
fn merge_tracks(base: &[Track], mem: &mut Vec<Track>, disk: &[Track]) {
    let base_keys: HashSet<_> = base.iter().map(track_key).collect();
    let disk_keys: HashSet<_> = disk.iter().map(track_key).collect();
    let mem_keys: HashSet<_> = mem.iter().map(track_key).collect();
    mem.retain(|t| !base_keys.contains(&track_key(t)) || disk_keys.contains(&track_key(t)));
    for track in disk {
        let key = track_key(track);
        if !mem_keys.contains(&key) && !base_keys.contains(&key) {
            mem.push(track.clone());
        }
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

        let (loaded, warning) = Playlists::load(&path).unwrap();
        assert!(warning.is_none());
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(loaded.items[0].name, "mix");
        assert_eq!(loaded.items[0].tracks[1].title, "título b");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn read_error_propagates() {
        // Un directorio en el lugar del archivo produce un error de lectura
        // distinto de NotFound: debe propagarse, no cargar vacío.
        let path = temp_path("es-directorio.json");
        fs::create_dir_all(&path).unwrap();
        assert!(Playlists::load(&path).is_err());
        let _ = fs::remove_dir(&path);
    }

    #[test]
    fn save_merges_external_playlists() {
        let path = temp_path("merge.json");
        let mut playlists = Playlists::default();
        let local = playlists.create("local").unwrap();
        playlists.add_track(local, track("mía"));
        playlists.save(&path).unwrap();

        // Otra instancia añade una playlist y toca la existente por detrás.
        let (mut other, _) = Playlists::load(&path).unwrap();
        other.create("externa").unwrap();
        other.add_track(0, track("ajena"));
        other.save(&path).unwrap();

        // Esta instancia guarda sin saberlo: conserva la playlist externa y,
        // en la de nombre compartido, fusiona las pistas de ambos lados.
        let pl = playlists.create("local-2").unwrap();
        playlists.add_track(pl, track("nueva"));
        playlists.save(&path).unwrap();

        let (loaded, warning) = Playlists::load(&path).unwrap();
        assert!(warning.is_none());
        let names: Vec<&str> = loaded.items.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["local", "local-2", "externa"]);
        let local = loaded.items.iter().find(|p| p.name == "local").unwrap();
        let ids: Vec<&str> = local.tracks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["mía", "ajena"]);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn save_respects_external_removals() {
        let path = temp_path("merge-remove.json");
        let mut playlists = Playlists::default();
        let mix = playlists.create("mix").unwrap();
        playlists.add_track(mix, track("x"));
        playlists.add_track(mix, track("y"));
        playlists.create("borrada").unwrap();
        playlists.save(&path).unwrap();

        // Otra instancia borra la playlist "borrada" y quita la pista "x".
        let (mut other, _) = Playlists::load(&path).unwrap();
        let idx = other.items.iter().position(|p| p.name == "borrada").unwrap();
        other.delete(idx).unwrap();
        other.remove_track(0, 0).unwrap();
        other.save(&path).unwrap();

        // Esta instancia añade "z" a "mix" desde su snapshot viejo y guarda.
        playlists.add_track(0, track("z"));
        playlists.save(&path).unwrap();

        let (loaded, _) = Playlists::load(&path).unwrap();
        assert!(
            !loaded.items.iter().any(|p| p.name == "borrada"),
            "la playlist eliminada externamente no resucita"
        );
        let mix = loaded.items.iter().find(|p| p.name == "mix").unwrap();
        let ids: Vec<&str> = mix.tracks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["y", "z"], "el borrado externo de pista se respeta y la adición local se conserva");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn corrupt_file_is_backed_up_and_loads_empty() {
        let path = temp_path("corrupt.json");
        fs::write(&path, "no json").unwrap();
        let (loaded, warning) = Playlists::load(&path).unwrap();
        assert!(loaded.items.is_empty());
        assert!(warning.is_some());
        assert!(path.with_extension("json.bak").exists());
        let _ = fs::remove_file(path.with_extension("json.bak"));
    }

    #[test]
    fn incompatible_version_is_backed_up() {
        let path = temp_path("version.json");
        fs::write(&path, r#"{"version": 42, "playlists": []}"#).unwrap();
        let (loaded, warning) = Playlists::load(&path).unwrap();
        assert!(loaded.items.is_empty());
        assert!(warning.is_some());
        let _ = fs::remove_file(path.with_extension("json.bak"));
    }
}
