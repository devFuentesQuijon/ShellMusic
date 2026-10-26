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

## CI

GitHub Actions (`.github/workflows/ci.yml`) corre en cada push/PR a `main`:

- **Los 3 SO** (Linux, macOS, Windows): build, `clippy -D warnings` y tests.
- **Reproducción real verificada en los 3 SO**: se instala mpv (+ffmpeg donde hace falta) y corre el test IPC local (`tests/mpv_ipc_local.rs`, sin red), lo que verifica la reproducción real incluido el named pipe de Windows.

La reproducción con red (streams de YouTube/SoundCloud) no es verificable en CI: el anti-bot de YouTube bloquea IPs de datacenter. Esa verificación es manual/local.

Además, un workflow nocturno (`nightly.yml`) corre a diario los tests que necesitan red (búsquedas reales en YouTube y SoundCloud, detalle de errores de mpv/yt-dlp); si falla una ejecución programada, se abre un issue automáticamente.

## Decisiones de diseño registradas

- **YouTube Data API v3: descartada.** No proporciona URLs de stream (la reproducción seguiría dependiendo de yt-dlp), exige que cada usuario cree un proyecto en Google Cloud con su API key y limita la búsqueda a 100 llamadas/día por proyecto. No aporta robustez al problema real.
- **MusicBrainz: descartado.** Aportaría identidad canónica de canción/artista, pero su rate limit de 1 petición/segundo lo hace inviable en el path crítico y el matching difuso reintroduce fragilidad. La normalización offline de artistas en estadísticas cubre el dolor real sin red.
- **Media keys en Windows/macOS: descartadas.** Los controles multimedia del SO exigen una ventana (HWND) o un event loop de GUI (AppDelegate), incompatibles con una TUI pura.
