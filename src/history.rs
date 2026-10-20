//! Historial de búsquedas: se navega con ↑/↓ desde la caja de búsqueda y se
//! persiste en `history.json` junto a favoritos y playlists. Las entradas
//! nunca se borran (solo se reordenan al repetir una búsqueda), así que la
//! fusión entre instancias es una simple unión.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const VERSION: u32 = 1;
pub const MAX_ENTRIES: usize = 50;

#[derive(Debug, Default, Serialize, Deserialize)]
struct HistoryFile {
    version: u32,
    entries: Vec<String>,
}

#[derive(Debug, Default)]
pub struct History {
    pub entries: Vec<String>,
    /// Contenido del archivo la última vez que se leyó o escribió.
    fingerprint: Option<String>,
}

impl History {
    pub fn path() -> Option<PathBuf> {
        Some(dirs::data_dir()?.join("shellmusic").join("history.json"))
    }

    /// Carga el historial; ante archivo corrupto o incompatible lo aparta a
    /// `.bak` y empieza vacío, devolviendo un aviso para la UI. Un error de
    /// lectura distinto de "no existe" se propaga.
    pub fn load(path: &Path) -> std::io::Result<(Self, Option<String>)> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Self::default(), None));
            }
            Err(err) => return Err(err),
        };
        let (mut history, warning) = match serde_json::from_str::<HistoryFile>(&text) {
            Ok(file) if file.version == VERSION => (
                Self {
                    entries: file.entries,
                    fingerprint: None,
                },
                None,
            ),
            _ => {
                let backup = path.with_extension("json.bak");
                let _ = fs::rename(path, &backup);
                (
                    Self::default(),
                    Some(format!(
                        "history.json incompatible o corrupto; se guardó copia en {}",
                        backup.display()
                    )),
                )
            }
        };
        history.entries.truncate(MAX_ENTRIES);
        history.fingerprint = Some(text);
        Ok((history, warning))
    }

    /// Registra una búsqueda: si ya existe se mueve al final (la más
    /// reciente); al superar el tope se descarta la más antigua.
    pub fn push(&mut self, query: &str) {
        if let Some(index) = self.entries.iter().position(|e| e == query) {
            self.entries.remove(index);
        }
        self.entries.push(query.to_string());
        if self.entries.len() > MAX_ENTRIES {
            self.entries.remove(0);
        }
    }

    /// Escritura atómica bajo lock: si el archivo cambió en disco (otra
    /// instancia), se fusionan las entradas externas nuevas al final.
    pub fn save(&mut self, path: &Path) -> std::io::Result<()> {
        use crate::state_file::{SaveLock, unique_tmp_path};

        let _lock = SaveLock::acquire(path)?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let disk = fs::read_to_string(path).ok();
        if disk != self.fingerprint
            && let Some(text) = disk
            && let Ok(file) = serde_json::from_str::<HistoryFile>(&text)
            && file.version == VERSION
        {
            for entry in file.entries {
                if !self.entries.contains(&entry) {
                    self.entries.push(entry);
                }
            }
            if self.entries.len() > MAX_ENTRIES {
                let excess = self.entries.len() - MAX_ENTRIES;
                self.entries.drain(..excess);
            }
        }
        let file = HistoryFile {
            version: VERSION,
            entries: self.entries.clone(),
        };
        let content = serde_json::to_string_pretty(&file)?;
        let tmp = unique_tmp_path(path);
        fs::write(&tmp, &content)?;
        fs::rename(&tmp, path)?;
        self.fingerprint = Some(content);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "shellmusic-history-test-{}-{}",
            std::process::id(),
            name
        ))
    }

    #[test]
    fn push_dedupes_and_moves_to_end() {
        let mut history = History::default();
        history.push("daft punk");
        history.push("rush");
        history.push("daft punk");
        assert_eq!(history.entries, ["rush", "daft punk"]);
    }

    #[test]
    fn push_caps_at_max_entries() {
        let mut history = History::default();
        for i in 0..MAX_ENTRIES + 5 {
            history.push(&format!("búsqueda {i}"));
        }
        assert_eq!(history.entries.len(), MAX_ENTRIES);
        assert_eq!(history.entries[0], "búsqueda 5");
        assert_eq!(history.entries.last().unwrap(), &format!("búsqueda {}", MAX_ENTRIES + 4));
    }

    #[test]
    fn persistence_round_trip() {
        let path = temp_path("roundtrip.json");
        let mut history = History::default();
        history.push("daft punk");
        history.push("yyz");
        history.save(&path).unwrap();

        let (loaded, warning) = History::load(&path).unwrap();
        assert!(warning.is_none());
        assert_eq!(loaded.entries, ["daft punk", "yyz"]);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn read_error_propagates() {
        let path = temp_path("es-directorio.json");
        fs::create_dir_all(&path).unwrap();
        assert!(History::load(&path).is_err());
        let _ = fs::remove_dir(&path);
    }

    #[test]
    fn save_merges_external_entries() {
        let path = temp_path("merge.json");
        let mut history = History::default();
        history.push("local");
        history.save(&path).unwrap();

        // Otra instancia añade una entrada por detrás.
        let (mut other, _) = History::load(&path).unwrap();
        other.push("externa");
        other.save(&path).unwrap();

        history.push("local-2");
        history.save(&path).unwrap();

        let (loaded, _) = History::load(&path).unwrap();
        assert!(loaded.entries.contains(&"local".to_string()));
        assert!(loaded.entries.contains(&"externa".to_string()));
        assert!(loaded.entries.contains(&"local-2".to_string()));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn corrupt_file_is_backed_up() {
        let path = temp_path("corrupt.json");
        fs::write(&path, "no json").unwrap();
        let (loaded, warning) = History::load(&path).unwrap();
        assert!(loaded.entries.is_empty());
        assert!(warning.is_some());
        assert!(path.with_extension("json.bak").exists());
        let _ = fs::remove_file(path.with_extension("json.bak"));
    }
}
