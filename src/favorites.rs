use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::providers::{ProviderKind, Track, track_key};

const VERSION: u32 = 1;

#[derive(Debug, Default, Serialize, Deserialize)]
struct FavoritesFile {
    version: u32,
    tracks: Vec<Track>,
}

/// Contenido del archivo para detectar escrituras externas entre carga y
/// guardado. Se compara contenido, no mtime: hay sistemas de archivos cuya
/// granularidad de mtime no distingue dos escrituras rápidas.
type Fingerprint = Option<String>;

fn capture_fingerprint(path: &Path) -> Fingerprint {
    fs::read_to_string(path).ok()
}

#[derive(Debug, Default)]
pub struct Favorites {
    pub tracks: Vec<Track>,
    fingerprint: Fingerprint,
    /// Claves del estado al cargar/guardar: base del merge a 3 bandas.
    base: HashSet<(ProviderKind, String)>,
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
        favorites.fingerprint = capture_fingerprint(path);
        favorites.base = favorites.tracks.iter().map(track_key).collect();
        Ok((favorites, warning))
    }

    /// Escritura atómica bajo lock de archivo: la sección comprobar-huella →
    /// fusionar → escribir → renombrar se serializa entre instancias, y el
    /// temporal es único por escritura. Si el archivo cambió en disco desde
    /// la última huella conocida (otra instancia), se hace merge a 3 bandas
    /// contra la base: las adiciones de cualquier lado se conservan y las
    /// eliminaciones de cualquier lado se respetan. Si el archivo existe
    /// pero no se puede releer, el guardado aborta en lugar de sobrescribir
    /// a ciegas estado que puede contener cambios externos.
    pub fn save(&mut self, path: &Path) -> std::io::Result<()> {
        use crate::state_file::{SaveLock, unique_tmp_path};

        let _lock = SaveLock::acquire(path)?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let changed = capture_fingerprint(path) != self.fingerprint
            // Huella vacía en ambos lados pero archivo presente: apareció
            // entre carga y guardado y no se ha podido leer (error, no ausencia).
            || (self.fingerprint.is_none() && path.exists());
        if changed {
            match fs::read_to_string(path) {
                Ok(text) => {
                    if let Ok(file) = serde_json::from_str::<FavoritesFile>(&text)
                        && file.version == VERSION
                    {
                        self.merge_external(file.tracks);
                    }
                }
                // Borrado externo: el guardado lo recrea con el estado en memoria.
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                // Existe pero ilegible: no sobrescribir sin conocer su contenido.
                Err(err) => return Err(err),
            }
        }
        let file = FavoritesFile {
            version: VERSION,
            tracks: self.tracks.clone(),
        };
        let tmp = unique_tmp_path(path);
        fs::write(&tmp, serde_json::to_string_pretty(&file)?)?;
        fs::rename(&tmp, path)?;
        self.fingerprint = capture_fingerprint(path);
        self.base = self.tracks.iter().map(track_key).collect();
        Ok(())
    }

    /// Merge a 3 bandas del estado en disco contra la base y la memoria:
    /// lo eliminado en cualquiera de los dos lados queda eliminado y lo
    /// añadido en cualquiera de los dos lados queda añadido.
    fn merge_external(&mut self, disk: Vec<Track>) {
        let disk_keys: HashSet<(ProviderKind, String)> = disk.iter().map(track_key).collect();
        let mem_keys: HashSet<(ProviderKind, String)> = self.tracks.iter().map(track_key).collect();
        // En base pero ausente del disco → eliminado externamente: fuera.
        // Ausente de base → añadido localmente: se queda.
        self.tracks
            .retain(|t| !self.base.contains(&track_key(t)) || disk_keys.contains(&track_key(t)));
        for track in disk {
            let key = track_key(&track);
            // En disco, ni en memoria ni en base → añadido externamente.
            // En base y en disco pero no en memoria → eliminado localmente.
            if !mem_keys.contains(&key) && !self.base.contains(&key) {
                self.tracks.push(track);
            }
        }
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
    fn save_respects_external_removals() {
        let path = temp_path("merge-remove.json");
        let mut favorites = Favorites::default();
        favorites.add(track("x"));
        favorites.save(&path).unwrap();

        // Otra instancia quita "x" y añade "y".
        let (mut other, _) = Favorites::load(&path).unwrap();
        other.toggle(&track("x"));
        other.add(track("y"));
        other.save(&path).unwrap();

        // Esta instancia añade "z" desde su snapshot viejo y guarda.
        favorites.add(track("z"));
        favorites.save(&path).unwrap();

        let (loaded, _) = Favorites::load(&path).unwrap();
        assert!(!loaded.contains(&track("x")), "la eliminación externa se respeta");
        assert!(loaded.contains(&track("y")), "la adición externa se conserva");
        assert!(loaded.contains(&track("z")), "la adición local se conserva");
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

    #[cfg(unix)]
    #[test]
    fn save_aborts_when_file_unreadable() {
        use std::os::unix::fs::PermissionsExt;

        let path = temp_path("ilegible.json");
        let mut favorites = Favorites::default();
        favorites.add(track("a"));
        favorites.save(&path).unwrap();

        // Otra instancia guarda cambios; después el archivo vuelve ilegible.
        let (mut other, _) = Favorites::load(&path).unwrap();
        other.add(track("externa"));
        other.save(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();

        // Este guardado no debe sobrescribir estado que no ha podido releer.
        favorites.add(track("b"));
        assert!(favorites.save(&path).is_err());

        // Al restaurar permisos, el contenido externo sigue intacto.
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let (loaded, _) = Favorites::load(&path).unwrap();
        assert!(loaded.contains(&track("externa")));
        assert!(!loaded.contains(&track("b")));
        let _ = fs::remove_file(&path);
    }

    #[cfg(unix)]
    #[test]
    fn save_aborts_when_file_appeared_unreadable() {
        use std::os::unix::fs::PermissionsExt;

        let path = temp_path("aparecido-ilegible.json");
        let (mut favorites, _) = Favorites::load(&path).unwrap(); // aún no existe

        // Otra instancia lo crea y el archivo queda ilegible antes de guardar.
        let mut other = Favorites::default();
        other.add(track("externa"));
        other.save(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();

        favorites.add(track("local"));
        assert!(favorites.save(&path).is_err());

        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let (loaded, _) = Favorites::load(&path).unwrap();
        assert!(loaded.contains(&track("externa")));
        assert!(!loaded.contains(&track("local")));
        let _ = fs::remove_file(&path);
    }
}
