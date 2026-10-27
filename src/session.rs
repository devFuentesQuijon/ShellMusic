//! Cola persistente entre sesiones: guarda la cola, la pista en curso y su
//! posición para reanudarlas en el próximo arranque.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::providers::Track;

const VERSION: u32 = 1;

#[derive(Debug, Default, Deserialize)]
struct SessionFile {
    version: u32,
    tracks: Vec<Track>,
    current: Option<usize>,
    position_secs: f64,
}

/// Vista prestada de la sesión para serializar sin clonar la cola.
#[derive(Serialize)]
struct SessionRef<'a> {
    version: u32,
    tracks: &'a [Track],
    current: Option<usize>,
    position_secs: f64,
}

/// Estado reproducible al cierre de la app; `position` acompaña a `current`.
#[derive(Debug, Default)]
pub struct Session {
    pub tracks: Vec<Track>,
    pub current: Option<usize>,
    pub position: Option<Duration>,
}

impl Session {
    pub fn path() -> Option<PathBuf> {
        Some(dirs::data_dir()?.join("shellmusic").join("session.json"))
    }

    /// Carga la sesión; ante archivo corrupto o de versión incompatible lo
    /// aparta a `.bak` y empieza vacío, devolviendo un aviso para la UI.
    /// Un error de lectura distinto de "no existe" se propaga (mismo criterio
    /// que favoritos/playlists: continuar vacío dejaría que un guardado
    /// posterior borrara el archivo).
    pub fn load(path: &Path) -> std::io::Result<(Self, Option<String>)> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Self::default(), None));
            }
            Err(err) => return Err(err),
        };
        match serde_json::from_str::<SessionFile>(&text) {
            Ok(file) if file.version == VERSION => {
                let current = file.current.filter(|index| *index < file.tracks.len());
                Ok((
                    Self {
                        tracks: file.tracks,
                        current,
                        // Solo tiene sentido reanudar la posición si hay pista.
                        position: current.map(|_| {
                            Duration::from_secs_f64(file.position_secs.max(0.0))
                        }),
                    },
                    None,
                ))
            }
            _ => {
                let backup = path.with_extension("json.bak");
                let _ = fs::rename(path, &backup);
                Ok((
                    Self::default(),
                    Some(format!(
                        "session.json incompatible o corrupto; se guardó copia en {}",
                        backup.display()
                    )),
                ))
            }
        }
    }

    /// Escritura atómica (temporal único + rename) bajo lock de archivo.
    /// Sin merge multi-instancia: la sesión la escribe la instancia activa y
    /// gana la última, a diferencia de favoritos/playlists.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        use crate::state_file::{SaveLock, unique_tmp_path};

        let _lock = SaveLock::acquire(path)?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let file = SessionRef {
            version: VERSION,
            tracks: &self.tracks,
            current: self.current,
            position_secs: self.position.map_or(0.0, |p| p.as_secs_f64()),
        };
        // JSON compacto: archivo interno que nadie edita a mano; el parseo
        // es idéntico y se ahorra CPU y bytes en cada cambio de pista.
        let tmp = unique_tmp_path(path);
        fs::write(&tmp, serde_json::to_string(&file)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
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
            duration: Some(Duration::from_secs(185)),
            source_url: format!("https://example.com/{id}"),
            provider: ProviderKind::Youtube,
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "shellmusic-session-test-{}-{}",
            std::process::id(),
            name
        ))
    }

    #[test]
    fn persistence_round_trip() {
        let path = temp_path("roundtrip.json");
        let session = Session {
            tracks: vec![track("a"), track("b")],
            current: Some(1),
            position: Some(Duration::from_secs_f64(42.5)),
        };
        session.save(&path).unwrap();

        let (loaded, warning) = Session::load(&path).unwrap();
        assert!(warning.is_none());
        assert_eq!(loaded.tracks.len(), 2);
        assert_eq!(loaded.tracks[1].title, "título b");
        assert_eq!(loaded.current, Some(1));
        assert_eq!(loaded.position, Some(Duration::from_secs_f64(42.5)));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn missing_file_loads_empty_without_warning() {
        let (loaded, warning) = Session::load(&temp_path("inexistente.json")).unwrap();
        assert!(loaded.tracks.is_empty());
        assert_eq!(loaded.current, None);
        assert_eq!(loaded.position, None);
        assert!(warning.is_none());
    }

    #[test]
    fn corrupt_file_is_backed_up_and_loads_empty() {
        let path = temp_path("corrupt.json");
        fs::write(&path, "{ esto no es json válido").unwrap();
        let (loaded, warning) = Session::load(&path).unwrap();
        assert!(loaded.tracks.is_empty());
        assert!(warning.is_some());
        assert!(!path.exists());
        assert!(path.with_extension("json.bak").exists());
        let _ = fs::remove_file(path.with_extension("json.bak"));
    }

    #[test]
    fn incompatible_version_is_backed_up() {
        let path = temp_path("version.json");
        fs::write(
            &path,
            r#"{"version": 99, "tracks": [], "current": null, "position_secs": 0.0}"#,
        )
        .unwrap();
        let (loaded, warning) = Session::load(&path).unwrap();
        assert!(loaded.tracks.is_empty());
        assert!(warning.is_some());
        let _ = fs::remove_file(path.with_extension("json.bak"));
    }

    #[test]
    fn out_of_range_current_is_dropped_with_its_position() {
        let path = temp_path("clamp.json");
        fs::write(
            &path,
            r#"{"version": 1, "tracks": [], "current": 5, "position_secs": 12.0}"#,
        )
        .unwrap();
        let (loaded, warning) = Session::load(&path).unwrap();
        assert!(warning.is_none());
        assert_eq!(loaded.current, None);
        assert_eq!(loaded.position, None);
        let _ = fs::remove_file(&path);
    }

    /// Cola grande: guardar+recargar 500 pistas debe ser rápido (la
    /// serialización es por referencia). Margen generoso (10×) para que el
    /// test no sea flaky en discos lentos o CI cargado.
    #[test]
    fn large_session_round_trip_is_fast() {
        let path = temp_path("grande.json");
        let session = Session {
            tracks: (0..500).map(|i| track(&i.to_string())).collect(),
            current: Some(499),
            position: Some(Duration::from_secs(30)),
        };
        let t0 = std::time::Instant::now();
        session.save(&path).unwrap();
        let (loaded, warning) = Session::load(&path).unwrap();
        let elapsed = t0.elapsed();
        assert!(warning.is_none());
        assert_eq!(loaded.tracks.len(), 500);
        assert_eq!(loaded.current, Some(499));
        assert!(
            elapsed < Duration::from_secs(5),
            "guardar+cargar 500 pistas tardó {elapsed:?}"
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn read_error_propagates() {
        let path = temp_path("es-directorio.json");
        fs::create_dir_all(&path).unwrap();
        assert!(Session::load(&path).is_err());
        let _ = fs::remove_dir(&path);
    }
}
