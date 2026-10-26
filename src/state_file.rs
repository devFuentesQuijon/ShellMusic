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
///
/// Anti-stale: si un proceso muere (`kill -9`) mientras escribe, el lock
/// queda huérfano y bloquearía todos los guardados futuros. Al encontrarlo,
/// se comprueba si el proceso dueño sigue vivo y la antigüedad del archivo:
/// si el dueño murió o el lock lleva más de 5 min, se considera huérfano y
/// se reemplaza.
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
                Ok(mut file) => {
                    // El PID del dueño viaja dentro del lock para poder
                    // detectar locks huérfanos de procesos matados.
                    use std::io::Write;
                    let _ = writeln!(file, "{}", std::process::id());
                    return Ok(Self { lock_path });
                }
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                    if is_stale_lock(&lock_path) {
                        // Lock huérfano: el dueño murió o es demasiado viejo.
                        let _ = fs::remove_file(&lock_path);
                        continue;
                    }
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

/// Un lock es huérfano si su dueño ya no existe o si lleva demasiado tiempo
/// (el guardado real nunca dura minutos; uno tan vieño es de un proceso
/// matado). El PID del dueño viaja dentro del lock.
fn is_stale_lock(lock_path: &Path) -> bool {
    const MAX_LOCK_AGE: Duration = Duration::from_secs(300);
    let Ok(metadata) = fs::metadata(lock_path) else {
        return true;
    };
    if let Ok(age) = metadata.modified().and_then(|m| {
        m.elapsed()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, ""))
    })
        && age > MAX_LOCK_AGE
    {
        return true;
    }
    owner_alive(lock_path)
        .map(|alive| !alive)
        .unwrap_or(false)
}

/// Lee el PID del dueño del lock y comprueba si el proceso sigue vivo.
fn owner_alive(lock_path: &Path) -> Option<bool> {
    let content = fs::read_to_string(lock_path).ok()?;
    let pid: u32 = content.trim().parse().ok()?;
    process_alive(pid)
}

#[cfg(unix)]
fn process_alive(pid: u32) -> Option<bool> {
    // kill(pid, 0): 0 = vivo, ESRCH = muerto.
    let result = unsafe { libc_kill(pid as i32, 0) };
    if result == 0 {
        Some(true)
    } else {
        Some(std::io::Error::last_os_error().raw_os_error() != Some(3)) // ESRCH=3: no existe
    }
}

#[cfg(not(unix))]
fn process_alive(_pid: u32) -> Option<bool> {
    // Sin una forma portable barata de consultar PIDs ajenos: solo la edad
    // del lock decide (cubierto por MAX_LOCK_AGE).
    None
}

#[cfg(unix)]
unsafe fn libc_kill(pid: i32, sig: i32) -> i32 {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    unsafe { kill(pid, sig) }
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

    #[test]
    fn stale_lock_from_dead_process_is_replaced() {
        let dir = std::env::temp_dir().join(format!("shellmusic-lock-stale-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let data = dir.join("state.json");
        // Simular un lock huérfano: PID imposible de un proceso muerto.
        let lock_path = data.with_extension("json.lock");
        fs::write(&lock_path, "999999999").unwrap();
        let _guard = SaveLock::acquire(&data).expect("el lock huérfano se reemplaza");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn live_lock_is_not_stale() {
        let dir = std::env::temp_dir().join(format!("shellmusic-lock-live-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let data = dir.join("state.json");
        let _guard = SaveLock::acquire(&data).unwrap();
        assert!(!is_stale_lock(&data.with_extension("json.lock")));
        let _ = fs::remove_dir_all(&dir);
    }
}
