//! Utilidades compartidas para el estado local en disco (favoritos y
//! playlists): lock de archivo para coordinar escritoras concurrentes y
//! nombres temporales únicos por escritura.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

const LOCK_RETRIES: u32 = 40;
const LOCK_RETRY_DELAY: Duration = Duration::from_millis(50);

/// Aleatorio por proceso sin dependencias: las claves de RandomState se
/// generan con semilla del SO en cada llamada.
pub(crate) fn random_suffix() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
}

/// Ruta de temporal única por escritura: dos instancias nunca comparten el
/// archivo temporal, así que writes/renames solapados no colisionan.
pub(crate) fn unique_tmp_path(path: &Path) -> PathBuf {
    path.with_extension(format!(
        "json.{}.{:016x}.tmp",
        std::process::id(),
        random_suffix()
    ))
}

/// Lock de archivo vía creación exclusiva (O_EXCL). Coordina la sección
/// crítica comprobar-huella → fusionar → escribir → renombrar entre
/// instancias de la app. Se libera al hacer drop.
#[derive(Debug)]
pub(crate) struct SaveLock {
    lock_path: PathBuf,
}

impl SaveLock {
    pub(crate) fn acquire(data_path: &Path) -> io::Result<Self> {
        let lock_path = data_path.with_extension("json.lock");
        if let Some(dir) = lock_path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut attempts = 0;
        loop {
            match fs::File::create_new(&lock_path) {
                Ok(_) => return Ok(Self { lock_path }),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                    attempts += 1;
                    if attempts >= LOCK_RETRIES {
                        return Err(io::Error::new(
                            io::ErrorKind::ResourceBusy,
                            format!(
                                "otra instancia está guardando {}; inténtalo de nuevo",
                                data_path.display()
                            ),
                        ));
                    }
                    std::thread::sleep(LOCK_RETRY_DELAY);
                }
                Err(err) => return Err(err),
            }
        }
    }
}

impl Drop for SaveLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_blocks_second_acquirer_until_released() {
        let dir = std::env::temp_dir().join(format!("shellmusic-lock-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let data = dir.join("state.json");

        let first = SaveLock::acquire(&data).unwrap();
        let data_clone = data.clone();
        let handle = std::thread::spawn(move || {
            SaveLock::acquire(&data_clone).map(|_| ())
        });
        std::thread::sleep(Duration::from_millis(150));
        assert!(!handle.is_finished(), "el segundo lock debe esperar");
        drop(first);
        handle.join().unwrap().unwrap();

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn held_lock_makes_save_fail_after_retries() {
        let dir = std::env::temp_dir().join(format!("shellmusic-lock2-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let data = dir.join("state.json");

        let _first = SaveLock::acquire(&data).unwrap();
        let err = SaveLock::acquire(&data).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::ResourceBusy);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unique_tmp_paths_differ() {
        let path = Path::new("favorites.json");
        assert_ne!(unique_tmp_path(path), unique_tmp_path(path));
    }
}
