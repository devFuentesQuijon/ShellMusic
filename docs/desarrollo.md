# Desarrollo

## Tests

Tests unitarios y de render (sin red, los de integración van con `#[ignore]`):

```sh
cargo test
```

Tests de integración (requieren `mpv`, `yt-dlp` y acceso a red):

```sh
cargo test -- --ignored
```

## Verificación E2E del checklist

`scripts/e2e_check.py` ejecuta la app real en una terminal emulada (pexpect + pyte) y comprueba de punta a punta el checklist de aceptación del SPEC §11 y la fase 18: arranque < 1 s, búsqueda con resultados, reproducción < 5 s, controles sin reiniciar, progreso real, errores en UI sin cerrar la app, URL pegada resuelta directamente y copia al portapapeles. Usa datos aislados en un directorio temporal y emula un terminal kitty (responde la query de capacidades gráficas; `--dumb-startup` añade la medición de arranque en un terminal sin respuesta).

```sh
cargo build --release
python3 scripts/e2e_check.py                 # requiere pexpect y pyte
python3 scripts/e2e_check.py --dumb-startup  # + terminal mudo <2s
```

Reproduce audio real y accede a red: si YouTube responde con bloqueo anti-bot, los tests de búsqueda fallan sin que sea regresión (ver `docs/anti-bot.md`).

## Instalador de Windows

`installer/smusic.iss` (Inno Setup 6) genera `smusic-windows-x86_64-setup.exe`: instalación por usuario (sin admin) en `%LOCALAPPDATA%\Programs\ShellMusic`, alta del PATH del usuario (HKCU) con limpieza al desinstalar, y un paso final que instala mpv y yt-dlp con winget si faltan. El workflow de release lo compila y publica en cada tag; la versión se inyecta con defines:

```sh
iscc /DAppVersion=0.3.1 /DVersionInfo=0.3.1.0 installer\smusic.iss
```

## CI

GitHub Actions (`.github/workflows/ci.yml`) corre en cada push/PR a `main`:

- **Los 3 SO** (Linux, macOS, Windows): build, `clippy -D warnings` y tests.
- **Reproducción real verificada en los 3 SO**: se instala mpv (+ffmpeg donde hace falta) y corre el test IPC local (`tests/mpv_ipc_local.rs`, sin red), lo que verifica la reproducción real incluido el named pipe de Windows.

La reproducción con red (streams de YouTube/SoundCloud) no es verificable en CI: el anti-bot de YouTube bloquea IPs de datacenter. Esa verificación es manual/local.

Además, un workflow nocturno (`nightly.yml`) corre a diario los tests que necesitan red (búsquedas reales en YouTube y SoundCloud, detalle de errores de mpv/yt-dlp); si falla una ejecución programada, se abre un issue automáticamente.

## Rendimiento

Decisiones tomadas en la auditoría de rendimiento (octubre 2026); conviene no
reintroducir los patrones originales por accidente:

- **Guardado de estadísticas diferido**: `play_queue_track_from` no escribe
  `stats.json` en cada cambio de pista; marca `stats_dirty` y el
  `periodic_save` del tick lo persiste cada ~30 s. Pérdida ante kill -9:
  estadísticas de <30 s (la sesión sí se guarda en cada cambio de pista).
- **Vista de estadísticas lazy**: `refresh_stats_view` (un O(n log n)) solo se
  ejecuta al abrir el overlay (`open_stats`), no en cada reproducción.
- **Serialización por referencia**: `Session::save` y `Stats::save` serializan
  vistas prestadas (`SessionRef`/`StatsRef`), sin clonar la cola o las
  entradas. `PlaybackStatus` deriva `PartialEq` para el redraw perezoso.
- **JSON compacto en estado interno** (`session.json`, `stats.json`,
  `history.json`): nadie los edita a mano y el parseo es idéntico.
  `favorites.json` y `playlists.json` conservan el pretty-print por si el
  usuario los abre.
- **Redraw perezoso**: el bucle principal de `app::run` solo repinta cuando un
  evento lo marca o cuando el `player.status()` difiere del último frame; la
  app en pausa no renderiza. El E2E (progreso, pausa, volumen) es el guardián
  de regresiones.
- **Chequeos de dependencias en paralelo**: `check_dependencies` lanza
  `mpv --version` y `yt-dlp --version` con `tokio::join!` (la versión de
  yt-dlp se reutiliza para el aviso de antigüedad).

## Decisiones de diseño registradas

- **YouTube Data API v3: descartada.** No proporciona URLs de stream (la reproducción seguiría dependiendo de yt-dlp), exige que cada usuario cree un proyecto en Google Cloud con su API key y limita la búsqueda a 100 llamadas/día por proyecto. No aporta robustez al problema real.
- **MusicBrainz: descartado.** Aportaría identidad canónica de canción/artista, pero su rate limit de 1 petición/segundo lo hace inviable en el path crítico y el matching difuso reintroduce fragilidad. La normalización offline de artistas en estadísticas cubre el dolor real sin red.
- **Media keys en Windows/macOS: descartadas.** Los controles multimedia del SO exigen una ventana (HWND) o un event loop de GUI (AppDelegate), incompatibles con una TUI pura.
