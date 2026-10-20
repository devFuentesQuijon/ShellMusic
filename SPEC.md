# ShellMusic — Especificación del proyecto

**Versión:** 0.1 (borrador para revisión)
**Estado:** Pendiente de aprobación
**Última actualización:** 2026-10-13

---

## 1. Resumen y objetivos

ShellMusic es un reproductor de música con interfaz de terminal (TUI) ultraliviana pero amigable, que permite buscar y reproducir contenido de plataformas de streaming de música desde un solo lugar.

### Objetivos

- Ofrecer una experiencia de búsqueda y reproducción rápida, íntegramente con teclado.
- Consumir recursos mínimos: adecuado para equipos modestos, sesiones SSH y uso junto a otras herramientas de terminal.
- Arquitectura extensible: añadir una nueva plataforma de música no debe requerir tocar la UI ni el núcleo.

### No-objetivos (fuera de alcance del MVP)

- Gestión de cola avanzada, favoritos y playlists (fases posteriores).
- Descarga permanente de contenido.
- Interfaz gráfica (GUI) o web.
- Integración con cuentas de usuario de las plataformas (bibliotecas personales, likes, etc.) en el MVP.

---

## 2. Stack técnico

| Componente | Tecnología | Justificación |
|---|---|---|
| Lenguaje | Rust 1.92+ | Binario único, bajo consumo, ecosistema TUI maduro (ncspot, termusic) |
| TUI | ratatui + crossterm | Estándar de facto, soporte Linux/macOS/Windows |
| Async | tokio | Búsquedas y reproducción no bloqueantes |
| Audio | mpv (proceso hijo, JSON IPC) | Seek, buffering y codecs resueltos; integración nativa con yt-dlp |
| Resolución YouTube | yt-dlp (proceso hijo) | Búsqueda (`ytsearch`) y resolución de streams sin API key |
| Datos/config | serde + serde_json + toml | Parseo de salidas JSON y archivo de configuración |
| Rutas por SO | dirs | Config en la ruta nativa de cada plataforma |
| Errores | anyhow | Propagación simple en aplicación (no librería) |

### Dependencias externas en runtime

| Dependencia | Uso | Instalación |
|---|---|---|
| mpv ≥ 0.30 | Motor de reproducción | Linux: `apt install mpv` · macOS: `brew install mpv` · Windows: `winget install mpv` o scoop |
| yt-dlp | Búsqueda y resolución de streams | Linux/macOS: gestor de paquetes o `pip install yt-dlp` · Windows: `winget install yt-dlp` |

Al arrancar, ShellMusic verifica que ambos ejecutables existen en el `PATH`. Si falta alguno, muestra un mensaje claro con instrucciones de instalación y termina con código de salida distinto de cero (nunca panic).

---

## 3. Plataformas soportadas

| SO | Estado MVP | Notas |
|---|---|---|
| Linux (x86_64) | Soportado, plataforma de desarrollo | Entorno verificado: Rust 1.92, mpv 0.37, yt-dlp 2026.08 |
| macOS (Intel/ARM) | Soportado | mpv y yt-dlp vía Homebrew; IPC por socket Unix |
| Windows (10/11) | Soportado | IPC de mpv por named pipe (abstracción por SO en el cliente IPC); se requiere Windows Terminal o similar para mejor experiencia |

### Rutas de configuración por SO (crate `dirs`)

- Linux: `~/.config/shellmusic/config.toml`
- macOS: `~/Library/Application Support/shellmusic/config.toml`
- Windows: `%APPDATA%\shellmusic\config.toml`

---

## 4. Requisitos funcionales (MVP)

| ID | Requisito | Prioridad |
|---|---|---|
| RF-1 | El usuario puede escribir una consulta de búsqueda en una caja de texto dedicada. | Alta |
| RF-2 | Al confirmar la búsqueda, se muestran hasta 15 resultados de YouTube Music con título, artista/canal y duración. | Alta |
| RF-3 | El usuario navega la lista de resultados con flechas (↑/↓) y selecciona con Enter. | Alta |
| RF-4 | Al seleccionar, la reproducción inicia y la barra de player muestra título, progreso y duración. | Alta |
| RF-5 | Controles de reproducción: play/pausa (Espacio), stop (s), volumen (+/-), siguiente (n) y anterior (p) dentro de la lista de resultados ya cargada. | Alta |
| RF-6 | La barra de progreso refleja la posición real, actualizándose al menos 2 veces por segundo. | Alta |
| RF-7 | `/` devuelve el foco a la caja de búsqueda desde cualquier panel; `q` sale de la aplicación. | Alta |
| RF-8 | Los errores (búsqueda sin resultados, stream caído, dependencia ausente) se muestran en la UI sin cerrar la aplicación. | Media |
| RF-9 | La configuración (volumen inicial, nº de resultados) se lee de `config.toml`; si no existe, se crea con valores por defecto. | Media |
| RF-10 | Cola de reproducción explícita en un tercer panel; `Tab`/`Shift+Tab` cicla el foco Búsqueda → Resultados → Cola. | Alta |
| RF-11 | En Resultados, `Enter` reproduce la pista al instante y la añade a la cola (sin duplicados); `a` añade sin interrumpir la reproducción. | Alta |
| RF-12 | En Cola: `d` elimina la seleccionada, `J`/`K` la mueven abajo/arriba, `Enter` salta a reproducirla. Si se elimina la pista que suena, se reproduce la que ocupa su lugar; si era la última, se detiene. | Alta |
| RF-13 | `n`/`p` navegan la cola (sin bucle: al final, `n` no hace nada). | Alta |
| RF-14 | Autoplay: al terminar una pista de forma natural (end-file `eof`), avanza automáticamente a la siguiente de la cola; `s` (stop) no dispara autoplay. | Alta |
| RF-15 | `f` marca/desmarca favorito sobre la selección de Resultados/Cola, o sobre la pista en reproducción si no hay selección; las favoritas se muestran con `★` en Resultados, Cola y Favoritos. | Alta |
| RF-16 | Panel de Favoritos como cuarta vista del ciclo `Tab`; `Enter` reproduce (y encola), `a` encola sin interrumpir, `d`/`f` quita de favoritos. | Alta |
| RF-17 | Los favoritos persisten en `favorites.json` en el dir de datos nativo (`dirs::data_dir()`), con escritura atómica (tmp + rename); archivo corrupto o de versión incompatible se aparta a `.bak` y se arranca con lista vacía sin panic. | Alta |
| RF-18 | `P` abre un overlay modal de playlists (lista con nombre y nº de pistas): `n` crea (input de nombre en el overlay), `d` elimina pidiendo confirmación (pulsar `d` dos veces), `Enter` abre el contenido, `Esc` cierra. | Alta |
| RF-19 | Vista de contenido de playlist (en el overlay): `Enter` reproduce la pista (y la añade a la cola, sin cerrar el overlay), `a` la encola sin interrumpir, `d` la quita, `J`/`K` reordenan, `A` reproduce la playlist entera (encola todo y suena la primera), `Esc` vuelve a la lista. | Alta |
| RF-20 | `A` sobre una pista (Resultados/Cola/Favoritos) abre el selector "añadir a playlist" (playlists existentes + "nueva playlist…"); dedup por (provider, id) dentro de la playlist. Nombres de playlist únicos: duplicado → error amable en la línea de estado. | Alta |
| RF-21 | Las playlists persisten en `playlists.json` (mismo dir de datos, mismo formato con `version`, escritura atómica, corrupto → `.bak` + aviso, sin panic). El overlay es modal: captura todas las teclas salvo Ctrl+C. | Alta |
| RF-22 | Visualizador de espectro opcional (tecla `v`, desactivado por defecto): barras animadas por FFT del audio del sistema vía monitor del sink de PipeWire/PulseAudio (`parec`, fallback `pw-cat`). Solo Linux. El subproceso de captura solo existe mientras está activo; con el visualizador apagado el tick de render es 500 ms y activo sube a `fps`. | Media |

### Flujo de usuario principal

1. `smusic` arranca → caja de búsqueda con foco.
2. Escribes "daft punk" + Enter → búsqueda asíncrona → lista de resultados.
3. Flechas + Enter → empieza a sonar; barra inferior muestra track y progreso.
4. `n`/`p` recorre los resultados como lista de reproducción implícita.
5. `/` para una nueva búsqueda; `q` para salir.

---

## 5. Requisitos no funcionales

| ID | Requisito | Métrica |
|---|---|---|
| RNF-1 | Arranque rápido | < 1 s hasta primera pantalla (sin contar búsqueda) |
| RNF-2 | Consumo mínimo | Binario release < 15 MB; RAM propia del proceso < 50 MB (mpv aparte) |
| RNF-3 | UI responsive | Usable en terminal de 80×24; layout adaptativo por encima |
| RNF-4 | Robustez | Ningún error de red, parseo o dependencia produce panic; se muestra en UI o en stderr con mensaje accionable |
| RNF-5 | Latencia percibida | La UI nunca se bloquea por búsquedas o carga de stream (todo async) |

---

## 6. Arquitectura

### Diagrama de componentes

```
┌──────────────────────────── ShellMusic ────────────────────────────┐
│                                                                    │
│   ┌─────────┐    eventos     ┌──────────────┐                      │
│   │   TUI   │◄──────────────►│     App      │                      │
│   │(ratatui)│   estado       │ (event loop) │                      │
│   └─────────┘                └──┬────────┬──┘                      │
│                                 │        │                         │
│                    ┌────────────▼─┐    ┌─▼─────────────┐           │
│                    │  Provider    │    │    Player     │           │
│                    │   (trait)    │    │    (trait)    │           │
│                    └──────┬───────┘    └───────┬───────┘           │
└───────────────────────────┼────────────────────┼───────────────────┘
                            │                    │
                    ┌───────▼───────┐    ┌───────▼───────┐
                    │    yt-dlp     │    │      mpv      │
                    │  (subproceso) │    │ (subproceso,  │
                    │               │    │   JSON IPC)   │
                    └───────────────┘    └───────────────┘
```

### Estructura de módulos

```
src/
├── main.rs            # entrada: runtime tokio, verifica dependencias, carga config
├── lib.rs             # re-exporta módulos (tests de integración)
├── app/               # estado global + bucle de eventos (teclado / tick / mensajes async)
│   ├── mod.rs         # bucle run() + mensajes internos (AppMsg)
│   ├── state.rs       # App, Focus, Overlay, NameContext y selección
│   ├── input.rs       # manejo de teclas por foco y overlay
│   └── playback.rs    # búsqueda, reproducción, cola y volumen
├── config.rs          # config nativa por SO, formato TOML, defaults
├── queue.rs           # cola de reproducción (añadir, eliminar, reordenar, actual)
├── favorites.rs       # favoritos locales (persistencia JSON atómica)
├── playlists.rs       # playlists locales (persistencia JSON atómica)
├── visualizer.rs      # captura de audio + FFT + bandas (espectro)
├── browsers.rs        # detección de navegadores instalados (cookies anti-bot)
├── providers/
│   ├── mod.rs         # trait MusicProvider + tipos Track + ProviderKind
│   ├── ytdlp.rs       # helper compartido: búsqueda yt-dlp (ytsearch/scsearch) y parseo
│   ├── youtube.rs     # YouTube Music vía yt-dlp
│   └── soundcloud.rs  # SoundCloud vía yt-dlp
├── player/
│   ├── mod.rs         # trait Player + tipos de estado (PlaybackStatus, PlayerEvent)
│   └── mpv.rs         # cliente JSON IPC de mpv (socket Unix / named pipe Windows)
└── tui/
    ├── mod.rs         # layout principal: búsqueda / resultados+cola+favoritos / barra player
    ├── search.rs      # input + lista de resultados
    ├── queue.rs       # panel de cola de reproducción
    ├── favorites.rs   # panel de favoritos
    ├── playlists.rs   # overlay modal de playlists
    ├── visualizer.rs  # franja de espectro (barras animadas)
    └── player_bar.rs  # progreso, volumen, track actual, posición en cola
```

### Contratos clave

```rust
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration: Option<Duration>,
    pub source_url: String,
    pub provider: ProviderKind,
}

#[async_trait]
pub trait MusicProvider: Send + Sync {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>>;
}

#[async_trait]
pub trait Player: Send + Sync {
    async fn play(&self, url: &str) -> Result<()>;
    async fn toggle_pause(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    async fn set_volume(&self, volume: u8) -> Result<()>;   // 0-100
    async fn status(&self) -> Result<PlaybackStatus>;        // posición, duración, pausado
}
```

**Regla de extensibilidad:** la TUI y `app.rs` solo conocen los traits. Añadir Spotify o SoundCloud = nuevo archivo en `providers/` que implementa `MusicProvider`; ningún cambio en UI.

### Máquina de estados de la UI

- `Searching` — foco en la caja de búsqueda (edición de texto).
- `Browsing` — foco en la lista de resultados (navegación y selección).
- La reproducción es ortogonal al foco: la barra de player siempre visible.

---

## 7. Atajos de teclado

| Tecla | Contexto | Acción |
|---|---|---|
| Texto + Enter | Búsqueda | Ejecutar búsqueda |
| Esc / Tab | Búsqueda | Pasar a resultados |
| Tab / Shift+Tab | Global (fuera de edición) | Ciclar foco: Búsqueda → Resultados → Cola → Favoritos |
| ↑ / ↓ (o k / j) | Resultados / Cola / Favoritos | Mover selección |
| Enter | Resultados | Reproducir selección (y añadirla a la cola si no está) |
| a | Resultados | Añadir a la cola sin interrumpir la reproducción |
| f | Resultados / Cola | Marcar / quitar favorito (o de la pista en reproducción si no hay selección) |
| Enter | Cola | Reproducir la pista seleccionada |
| d | Cola | Eliminar de la cola (si es la que suena, salta a la siguiente o detiene) |
| J / K | Cola | Mover la pista seleccionada abajo / arriba |
| Enter | Favoritos | Reproducir (y añadir a la cola) |
| a | Favoritos | Añadir a la cola sin interrumpir |
| d / f | Favoritos | Quitar de favoritos |
| Espacio | Resultados / Cola / Favoritos | Play / pausa |
| s | Resultados / Cola / Favoritos | Stop (sin autoplay posterior) |
| + / - | Resultados / Cola / Favoritos | Volumen arriba / abajo |
| n / p | Resultados / Cola / Favoritos | Siguiente / anterior pista de la cola (sin bucle) |
| / | Resultados / Cola / Favoritos | Foco a búsqueda |
| q | Resultados / Cola / Favoritos | Salir |
| P | Resultados / Cola / Favoritos | Abrir gestor de playlists (overlay modal) |
| A | Resultados / Cola / Favoritos | Añadir la pista seleccionada (o la que suena) a una playlist |
| m | Resultados / Cola / Favoritos | Cambiar de proveedor de búsqueda (cicla los habilitados en `[general] providers`) |
| v | Resultados / Cola / Favoritos | Activar / desactivar el visualizador de espectro |
| n / d / Enter / Esc | Overlay: lista de playlists | Nueva / eliminar (d dos veces) / abrir contenido / cerrar |
| Enter / a / A / d / J / K / Esc | Overlay: contenido | Reproducir / encolar / reproducir toda / quitar / reordenar / volver |
| Enter / Esc | Overlay: añadir a playlist | Añadir a la seleccionada (o crear nueva) / cancelar |
| Ctrl+C | Global | Salir |

---

## 8. Configuración (`config.toml`)

```toml
[general]
results_per_search = 15   # 1-50
providers = ["youtube", "soundcloud"]   # proveedores habilitados; la tecla m cicla entre ellos
default_provider = "youtube"            # proveedor activo al arrancar (debe estar en la lista)

[audio]
initial_volume = 70       # 0-100

[youtube]
cookies_from_browser = "auto"   # "auto": primero sin cookies y, si YouTube bloquea, reintenta con cada navegador instalado; "firefox"/"chrome"/...: solo ese; "": nunca usar cookies
player_client = "android"          # cliente de YouTube que yt-dlp emula ("android" evita el 403 por PO Token; "" = cliente por defecto)
remote_components = "ejs:github"   # permite a yt-dlp resolver los retos JS de firma ("" = desactivado)

[visualizer]
enabled = false   # espectro animado (solo Linux; captura el audio del sistema)
bands = 24        # 8-64
fps = 20          # 5-60
```

Si el archivo no existe al primer arranque, se crea con estos valores.

### Visualizador (RF-22)

Franja de barras animadas (FFT de 2048 muestras, ventana Hann, 24 bandas log-espaciadas 30 Hz–16 kHz, escala dB, suavizado ataque rápido/caída lenta) sobre la barra del player. **Solo Linux**: captura el monitor del sink por defecto con `parec` (fallback `pw-cat`), por lo que recoge TODO el audio del sistema, no solo el de ShellMusic. En macOS/Windows, `v` muestra un aviso y no activa nada. El subproceso de captura se crea al activar y se mata al desactivar o salir; apagado no consume recursos de captura ni de redibujado extra.

### Nota sobre el bloqueo anti-bot de YouTube

YouTube puede bloquear la resolución de streams desde ciertas IPs (HTTP 429 / "Sign in to confirm you're not a bot" al buscar, o HTTP 403 al abrir el stream de googlevideo). Las claves de la sección `[youtube]` se traducen a `--ytdl-raw-options` de mpv (aplicadas en caliente vía IPC en cada `loadfile`):

- `cookies_from_browser`:
  - `"auto"` (por defecto): el primer intento se hace sin cookies; si falla con un error de bloqueo (patrones "Sign in to confirm", "HTTP Error 429/403", "Requested format is not available", "cookies"), se reintenta con cada navegador instalado que yt-dlp soporta (detección por binario en PATH y directorio de perfil por SO; firefox primero). El navegador que funciona se cachea en memoria durante la sesión y las siguientes pistas lo usan directamente. Si todos los intentos fallan, la UI sugiere iniciar sesión en YouTube desde algún navegador instalado (basta una sesión de visitante).
  - `"firefox"`, `"chrome"`, etc.: usa solo ese navegador, sin auto-detección.
  - `""`: nunca usa cookies ni reintenta.
- `player_client`: emular el cliente Android evita la exigencia de GVS PO Token que produce el 403 en los clientes web/visionos.
- `remote_components`: permite a yt-dlp descargar el solucionador de retos JS de firma (requiere un runtime JS como bun, deno o node instalado).

Las claves ausentes en un `config.toml` preexistente toman su valor por defecto (compatibilidad hacia atrás).

---

## 9. Distribución

- **MVP**: `cargo build --release` por plataforma; binario único `smusic` (instalable con `cargo install --path .`).
- **CI**: GitHub Actions (`.github/workflows/ci.yml`) — build, clippy (`-D warnings`) y tests en Linux, macOS y Windows en cada push/PR a `main`. En Linux se instalan además mpv+ffmpeg y corre el test de integración IPC local (`tests/mpv_ipc_local.rs`, sin red).
- **Integración nocturna**: `.github/workflows/nightly.yml` corre a diario (04:23 UTC, también manual con `workflow_dispatch`) los tests que necesitan red: búsqueda real en YouTube (`tests/youtube_search.rs`), búsqueda real en SoundCloud (`tests/soundcloud_search.rs`) y detalle de error de mpv/yt-dlp (`tests/mpv_ipc.rs::mpv_reports_stderr_detail_on_failure`). Si falla una ejecución programada, se abre un issue automáticamente. Quedan excluidos a propósito los tests que requieren navegador con sesión de YouTube, reproducción real (el anti-bot bloquea IPs de datacenter) o stack de audio (PipeWire/PulseAudio).
- **Releases**: al pushear un tag `v*` (`.github/workflows/release.yml`) se compilan y publican en GitHub Releases los binarios `smusic-linux-x86_64`, `smusic-macos-aarch64` y `smusic-windows-x86_64.exe`.
- **Futuro**: publicación en crates.io, fórmula Homebrew, paquete scoop.

---

## 10. Roadmap por fases

| Fase | Contenido |
|---|---|
| **0** | Esta especificación (aprobación previa al código) |
| **1 (MVP)** | Búsqueda + reproducción YouTube Music, Linux/macOS/Windows — **completada** |
| **2** | Cola de reproducción gestionable (añadir, quitar, reordenar, autoplay) — **completada** |
| **3** | Favoritos locales — **completada** |
| **4** | Playlists locales — **completada** |
| 5 | Provider Spotify — **pospuesta**: desde feb-2026 las apps nuevas en Development Mode exigen que cada usuario cree su propia app de desarrollador y tenga Premium activo ([guía de migración](https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide)); fricción excesiva para el usuario objetivo |
| **6** | Provider SoundCloud (búsqueda vía yt-dlp `scsearch`) + selector de proveedor en la TUI (tecla `m`, `[general] providers` / `default_provider`) — **completada** |
| 7 | Provider Apple Music — **pospuesta**: misma razón que la fase 5, agravada por la cuenta de desarrollador de pago |
| 8 | Extras: caché de búsquedas, modo radio, publicación en gestores de paquetes |

---

## 11. Criterios de aceptación del MVP

Checklist verificable end-to-end:

- [ ] `smusic` arranca en < 1 s y muestra la caja de búsqueda con foco.
- [ ] Buscar "daft punk" muestra ≥ 10 resultados con título, artista y duración.
- [ ] Enter sobre un resultado inicia reproducción audible en < 5 s.
- [ ] Play/pausa, stop, volumen y siguiente/anterior funcionan sin reiniciar.
- [ ] La barra de progreso refleja la posición real del track.
- [ ] Sin mpv o yt-dlp en el PATH, la app muestra un mensaje claro con instrucciones y sale sin panic.
- [ ] Una búsqueda sin resultados o una URL caída muestran el error en la UI sin cerrar la app.
- [ ] `cargo clippy` sin warnings y `cargo build --release` exitoso en Linux.
- [ ] El código no asume rutas ni sockets exclusivos de Unix (abstracción IPC por SO presente, aunque la verificación en macOS/Windows puede quedar documentada como pendiente si no hay hardware disponible).
