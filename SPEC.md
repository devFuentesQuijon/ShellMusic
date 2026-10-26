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
- Descarga permanente de contenido (ShellMusic hace streaming; ver el aviso legal del README).
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
| Linux (x86_64) | Soportado, plataforma de desarrollo | Entorno verificado: Rust 1.92, mpv 0.37, yt-dlp 2026.08. Todo verificado localmente, incluido MPRIS |
| macOS (Intel/ARM) | Soportado | mpv y yt-dlp vía Homebrew; IPC por socket Unix. Verificado en CI: build + reproducción local (sin red). Media controls: no (requieren AppDelegate de GUI) |
| Windows (10/11) | Soportado | IPC de mpv por named pipe (abstracción por SO en el cliente IPC); se requiere Windows Terminal o similar para mejor experiencia. Verificado en CI: build + reproducción local (sin red). Media controls: no (requieren HWND de ventana). Carátula: halfblocks |

La reproducción con red (streams de YouTube/SoundCloud) no es verificable en CI en ningún SO: el anti-bot de YouTube bloquea IPs de datacenter. Esa verificación es manual/local.

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
| RF-22 | Con la caja de búsqueda enfocada, `↑`/`↓` recorren el historial de búsquedas (estilo shell: al bajar de la más reciente se restaura el borrador en curso). Persisten las últimas 50 en `history.json` (mismo dir de datos, mismo formato y escritura atómica que favoritos/playlists). | Media |
| RF-23 | `←`/`→` hacen seek relativo dentro de la pista (±5 s; con Shift ±30 s) vía `Player::seek`; sin reproducción activa se avisa en la línea de estado. | Alta |
| RF-24 | `l` cicla el modo de repetición (off → toda la cola → pista actual), aplicado en el autoplay al terminar la pista de forma natural: `all` vuelve a la primera de la cola, `one` repite la pista; la barra del reproductor muestra `rep:all`/`rep:1`. La radio solo entra con repetición off. | Alta |
| RF-25 | `S` baraja la cola en el sitio manteniendo la pista en reproducción en su posición. | Media |
| RF-26 | `?` abre un overlay modal de ayuda con los atajos por contexto (se cierra con Esc, `?` o Enter); el pie de cada panel apunta a `? ayuda`. | Media |
| RF-27 | Prefetch de radio: al empezar a sonar la última pista de la cola (radio activa, repetición off) se lanza la expansión en segundo plano y las pistas nuevas se encolan sin interrumpir; al terminar la pista la siguiente entra sin pausa. Si la búsqueda falla o no hay nada nuevo, se reintenta al agotarse la cola (comportamiento clásico). | Alta |
| RF-28 | Navegación en listas largas: `PageUp`/`PageDown` saltan una página (la altura visible real del panel, fijada por el render) y `g`/`G` van al inicio/fin, en Resultados, Cola, Favoritos y las listas de los overlays de playlists. | Media |
| RF-29 | Cola persistente entre sesiones: al salir y en cada cambio de pista se guarda `session.json` (pistas, pista en curso y posición) en el dir de datos, con escritura atómica y corrupto → `.bak` + aviso; al arrancar se restaura la cola y se reanuda la pista desde la posición guardada. | Media |
| RF-30 | Temas de color en `[theme]` de `config.toml` (roles `accent`, `highlight`, `error`, `muted`, `progress`; nombres, `#rrggbb` o índice 0-255); valor inválido → color por defecto con aviso en la línea de estado, sin panic. | Baja |
| RF-31 | La pestaña del terminal muestra «ShellMusic» en reposo y «♪ artista — título · ShellMusic» durante la reproducción (OSC 2); al salir se restablece el título del terminal. | Baja |
| RF-32 | `X` exporta playlists y/o favoritos a JSON (menú con ámbito + ruta con `~`); en la vista de contenido de una playlist, `X` la exporta a M3U extendido. `I` importa según extensión: `.json` fusiona favoritos (dedupe) y playlists (nombre existente → merge de pistas, nuevo → se crea); `.m3u`/`.m3u8` crea una playlist con el nombre del archivo (URLs de YouTube extraen id; `#EXTINF` aporta título/artista/duración). Errores en la línea de estado, sin panic. | Media |
| RF-33 | Estadísticas locales de escucha: cada arranque de pista incrementa su conteo en `stats.json` (mismo dir de datos, escritura atómica, corrupto → `.bak`). `t` abre el overlay «Tus más escuchadas»: top pistas por reproducciones (`Enter` reproduce, `a` encola) y top artistas (se alterna con `Tab`). | Media |
| RF-34 | Notificación de escritorio al cambiar de pista (`notify-rust`, `[integrations] notifications`); el fallo de una notificación nunca se muestra en la UI ni interrumpe la app. | Media |
| RF-35 | MPRIS en Linux (`souvlaki`, backend zbus): teclas multimedia, widgets del escritorio y KDE Connect controlan play/pausa, stop, siguiente/anterior, seek, posición y volumen; metadata (título, artista, duración) y estado de reproducción se publican al cambiar. Sin D-Bus de sesión (SSH/headless) la app arranca igualmente con un aviso. En macOS/Windows no hay media controls: exigen ventana/event loop de GUI, incompatibles con una TUI (limitación documentada). | Alta |
| RF-36 | Autocompletado de la búsqueda desde el historial local: con la caja enfocada se muestra el sufijo de la entrada más reciente con ese prefijo como "ghost text" atenuado y `→` lo acepta. Sin red, instantáneo. | Media |
| RF-37 | `L` abre un overlay con la letra de la pista en reproducción (o la seleccionada) vía lrclib.net (sin API key; se elige la entrada con letra cuya duración más se acerque); scroll con ↑/↓, PgUp/PgDn y g/G; errores de red en el propio overlay, sin panic. | Media |
| RF-38 | `i` abre el panel «sonando ahora»: metadata ampliada (proveedor, duración/posición, volumen, escuchas, posición en cola) y carátula de YouTube (`i.ytimg.com`) renderizada con kitty/sixel si el terminal lo soporta, con fallback a halfblocks (`ratatui-image`). SoundCloud queda sin carátula. | Baja |
| RF-39 | Ante bloqueo anti-bot, la reproducción rota la combinación `player_client` × cookies: el cliente configurado primero, luego la cadena interna `ios`/`tv`/`web_embedded`, cada uno con su ronda de cookies; la combinación ganadora se cachea en memoria durante la sesión y, si deja de funcionar, se descarta y se recorre el plan completo de nuevo. | Alta |
| RF-40 | Si el plugin bgutil PO Token de yt-dlp está instalado (paquete pip o carpeta de plugins), con `[youtube] pot_provider = "auto"` se prueba primero el cliente por defecto de yt-dlp (el plugin suele dar mejores streams que forzar `android`); `"off"` lo ignora. El mensaje de bloqueo final sugiere instalarlo. | Media |
| RF-41 | Al arrancar, si `yt-dlp --version` tiene más de 90 días se muestra un aviso persistente en la línea de estado sugiriendo actualizarlo (los parches anti-bot llegan vía releases de yt-dlp); el arranque no se interrumpe y un formato irreconocible no avisa. | Media |
| RF-42 | Si el proceso mpv muere en marcha, la app lo respawnea sola con el volumen vigente (máx. 3 respawns en 60 s, con guarda anti-bucle y aviso «mpv se cerró; reiniciándolo…» en la línea de estado). | Alta |
| RF-43 | Los resultados de búsqueda llevan epoch: una búsqueda lanzada antes de otra no puede sobrescribir los resultados de la más reciente cuando llega tarde. | Media |
| RF-44 | Los fallos transitorios de yt-dlp (timeout, 502) se reintentan una vez tras 1,5 s antes de propagar el error a la UI. | Media |
| RF-45 | La sesión (cola + pista + posición) y las estadísticas pendientes se guardan periódicamente cada ~30 s durante la reproducción: un `kill -9` pierde como máximo 30 s. | Media |
| RF-46 | Los locks de guardado detectan huérfanos: si el proceso dueño murió (PID inexistente) o el lock lleva más de 5 min, se reemplaza en lugar de bloquear todos los guardados futuros. | Alta |
| RF-47 | Las estadísticas agrupan artistas por forma canónica offline («Daft Punk - Topic», «VEVO», «feat.»… se unifican), mostrando la forma original más frecuente; no muta datos persistidos. | Baja |
| RF-48 | Argumentos de línea de comandos: `--theme <nombre>` aplica un tema empaquetado (dracula, frankenstein, bride, wolfman, mummy, invisible, lagoon, thriller) con prioridad sobre `config.toml` solo en esa ejecución; nombre desconocido → error con la lista de disponibles. `--play <búsqueda>` lanza la búsqueda al arrancar y reproduce la primera coincidencia automáticamente. `-h`/`--help` muestra el uso. Los presets son fuente única de verdad en `src/theme.rs`; `docs/temas.md` es su vista legible. | Media |

Nota sobre la YouTube Data API v3 (evaluada y descartada): no proporciona URLs de stream (la reproducción seguiría dependiendo de yt-dlp), exige que cada usuario cree un proyecto en Google Cloud con su API key (misma fricción que llevó a posponer Spotify en la fase 5) y limita la búsqueda a 100 llamadas/día por proyecto. No aporta robustez al problema real.

Nota sobre MusicBrainz (evaluado y descartado): aportaría identidad canónica de canción/artista, pero su rate limit de 1 petición/segundo lo hace inviable en el path crítico, el matching difuso reintroduce fragilidad y la deduplicación por proveedor es intencional (dos subidas son dos tracks). La normalización offline de artistas en estadísticas (quitar «- Topic», «VEVO», «feat.») cubre el dolor real sin red.

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
├── browsers.rs        # detección de navegadores instalados (cookies anti-bot)
├── history.rs         # historial de búsquedas (persistencia JSON atómica)
├── lyrics.rs          # letras de canciones vía lrclib.net (reqwest)
├── session.rs         # cola persistente entre sesiones (persistencia JSON atómica)
├── stats.rs           # estadísticas de escucha (conteos por pista/artista)
├── portable.rs        # exportar/importar playlists y favoritos (JSON/M3U)
├── system.rs          # integración con el SO: notificaciones y MPRIS (Linux)
├── theme.rs           # tema de color resuelto desde [theme] de la config
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
    ├── stats.rs       # overlay de estadísticas («tus más escuchadas») y exportar/importar
    ├── nowplaying.rs  # overlay de letras y panel «sonando ahora» (carátula)
    ├── help.rs        # overlay modal de ayuda de atajos (tecla ?)
    └── player_bar.rs  # progreso, volumen, track actual, posición en cola, repetición
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
    async fn play(&self, url: &str, start: Option<Duration>) -> Result<()>;  // start: reanudar desde esa posición
    async fn toggle_pause(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    async fn seek(&self, seconds: i32) -> Result<()>;        // salto relativo en la pista
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
| ↑ / ↓ | Búsqueda | Navegar el historial de búsquedas (con borrador restaurable) |
| Esc / Tab | Búsqueda | Pasar a resultados |
| Tab / Shift+Tab | Global (fuera de edición) | Ciclar foco: Búsqueda → Resultados → Cola → Favoritos |
| → | Búsqueda | Aceptar el autocompletado del historial (ghost text) |
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
| ← / → | Resultados / Cola / Favoritos | Seek ∓5 s (con Shift: ∓30 s) |
| + / - | Resultados / Cola / Favoritos | Volumen arriba / abajo |
| n / p | Resultados / Cola / Favoritos | Siguiente / anterior pista de la cola (sin bucle) |
| l | Resultados / Cola / Favoritos | Ciclar repetición: off → toda la cola → pista actual |
| S | Resultados / Cola / Favoritos | Barajar la cola (la pista actual conserva su posición) |
| ? | Resultados / Cola / Favoritos | Overlay modal de ayuda de atajos (Esc/? cierra) |
| PgUp / PgDn | Listas (resultados, cola, favoritos, playlists) | Saltar una página |
| g / G | Listas (resultados, cola, favoritos, playlists) | Ir al inicio / al final |
| / | Resultados / Cola / Favoritos | Foco a búsqueda |
| q | Resultados / Cola / Favoritos | Salir |
| P | Resultados / Cola / Favoritos | Abrir gestor de playlists (overlay modal) |
| A | Resultados / Cola / Favoritos | Añadir la pista seleccionada (o la que suena) a una playlist |
| m | Resultados / Cola / Favoritos | Cambiar de proveedor de búsqueda (cicla los habilitados en `[general] providers`) |
| t | Resultados / Cola / Favoritos | Overlay «Tus más escuchadas» (estadísticas; `Tab` alterna pistas/artistas, `Enter` reproduce, `a` encola, `Esc` cierra) |
| X | Resultados / Cola / Favoritos | Exportar playlists/favoritos a JSON (menú de ámbito + ruta); en contenido de playlist: exportarla a M3U |
| I | Resultados / Cola / Favoritos | Importar desde `.json` (fusiona) o `.m3u`/`.m3u8` (crea playlist) |
| L | Resultados / Cola / Favoritos | Overlay con la letra de la pista que suena (o la seleccionada) |
| i | Resultados / Cola / Favoritos | Panel «sonando ahora» con metadata y carátula |
| r | Resultados / Cola / Favoritos | Activar / desactivar el modo radio (al empezar la última pista de la cola, encola pistas relacionadas por adelantado) |
| R | Resultados / Cola / Favoritos | Ciclar el sub-modo de radio: artista → similares (mix RD de YouTube) → random (artista al azar de favoritos) → random género (palabra clave al azar) → rarezas (deep cuts del artista) |
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
player_client = "android"          # cliente de YouTube que yt-dlp emula primero; si bloquea, se rota por ios → tv → web_embedded
remote_components = "ejs:github"   # permite a yt-dlp resolver los retos JS de firma ("" = desactivado)
pot_provider = "auto"              # "auto": usar el plugin bgutil PO Token de yt-dlp si está instalado (pip install bgutil-ytdlp-pot-provider); "off": ignorarlo

[cache]
enabled = true      # caché en memoria de búsquedas (solo dura la sesión)
ttl_minutes = 15    # 1-120

[radio]
enabled = false     # al agotarse la cola, encola pistas relacionadas (se alterna con la tecla r)
mode = "artista"    # artista | similares | random | random-genero | rarezas (se cicla con R)

[theme]
accent = "cyan"        # bordes del panel enfocado y títulos de sección
highlight = "yellow"   # mensajes informativos y teclas en la ayuda
error = "red"          # mensajes de error
muted = "darkgray"     # línea de hints y texto secundario
progress = "green"     # barra de progreso

[integrations]
notifications = true    # notificación de escritorio al cambiar de pista
media_controls = true   # MPRIS en Linux: teclas multimedia, widgets, KDE Connect
```

Los colores aceptan nombres (`cyan`, `lightred`, `darkgray`…), hex (`#rrggbb`) e índices de paleta (0-255); un valor inválido cae al color por defecto con un aviso en la línea de estado.

Si el archivo no existe al primer arranque, se crea con estos valores.

### Nota sobre el bloqueo anti-bot de YouTube

YouTube puede bloquear la resolución de streams desde ciertas IPs (HTTP 429 / "Sign in to confirm you're not a bot" al buscar, o HTTP 403 al abrir el stream de googlevideo). Las claves de la sección `[youtube]` se traducen a `--ytdl-raw-options` de mpv (aplicadas en caliente vía IPC en cada `loadfile`):

- `cookies_from_browser`:
  - `"auto"` (por defecto): el primer intento se hace sin cookies; si falla con un error de bloqueo (patrones "Sign in to confirm", "HTTP Error 429/403", "Requested format is not available", "cookies"), se reintenta con cada navegador instalado que yt-dlp soporta (detección por binario en PATH y directorio de perfil por SO; firefox primero).
  - `"firefox"`, `"chrome"`, etc.: usa solo ese navegador, sin auto-detección.
  - `""`: nunca usa cookies ni reintenta.
- `player_client`: el cliente configurado se intenta primero; si todas las combinaciones de cookies fallan con bloqueo, se rota por la cadena `ios` → `tv` → `web_embedded` (cada cliente con su ronda de cookies). La combinación cliente × cookies que funciona se cachea en memoria durante la sesión; si deja de funcionar, se descarta y se recorre el plan completo de nuevo.
- `pot_provider`: con `"auto"` y el plugin `bgutil-ytdlp-pot-provider` instalado (`pip install bgutil-ytdlp-pot-provider` o carpeta de plugins de yt-dlp), el primer intento usa el cliente por defecto de yt-dlp con PO Token, que suele dar mejores streams que forzar `android`.
- `remote_components`: permite a yt-dlp descargar el solucionador de retos JS de firma (requiere un runtime JS como bun, deno o node instalado).
- Antigüedad de yt-dlp: los parches anti-bot llegan vía releases de yt-dlp; si la versión instalada tiene más de 90 días, la app lo avisa al arrancar (sin bloquear).

Si toda la cadena falla, el mensaje de bloqueo sugiere las tres salidas: iniciar sesión en YouTube desde un navegador instalado (basta sesión de visitante), instalar el plugin PO Token o actualizar yt-dlp.

Las claves ausentes en un `config.toml` preexistente toman su valor por defecto (compatibilidad hacia atrás).

---

## 9. Distribución

- **MVP**: `cargo build --release` por plataforma; binario único `smusic` (instalable con `cargo install --path .`). Página de manual `docs/smusic.1` (se instala copiándola a `man1`; la futura fórmula Homebrew/scoop la incluirá).
- **CI**: GitHub Actions (`.github/workflows/ci.yml`) — build, clippy (`-D warnings`) y tests en Linux, macOS y Windows en cada push/PR a `main`. En los tres SO se instala mpv (+ffmpeg donde hace falta) y corre el test de integración IPC local (`tests/mpv_ipc_local.rs`, sin red), lo que verifica la reproducción real — incluido el named pipe de Windows.
- **Integración nocturna**: `.github/workflows/nightly.yml` corre a diario (04:23 UTC, también manual con `workflow_dispatch`) los tests que necesitan red: búsqueda real en YouTube (`tests/youtube_search.rs`), búsqueda real en SoundCloud (`tests/soundcloud_search.rs`) y detalle de error de mpv/yt-dlp (`tests/mpv_ipc.rs::mpv_reports_stderr_detail_on_failure`). Si falla una ejecución programada, se abre un issue automáticamente. Quedan excluidos a propósito los tests que requieren navegador con sesión de YouTube, reproducción real (el anti-bot bloquea IPs de datacenter) o stack de audio (PipeWire/PulseAudio).
- **Releases**: al pushear un tag `v*` (`.github/workflows/release.yml`) se compilan y publican en GitHub Releases los binarios `smusic-linux-x86_64`, `smusic-macos-aarch64` y `smusic-windows-x86_64.exe`.
- **Futuro**: publicación en gestores de paquetes. Los metadatos de crates.io ya están en `Cargo.toml` (`repository`, `homepage`, `readme`, `keywords`, `categories`, `exclude` de docs/scripts/CI) y `cargo package --list` produce un paquete limpio; quedan como pasos manuales del autor: `cargo publish` (requiere cuenta en crates.io), fórmula Homebrew (tap propio o PR a homebrew-core) y manifiesto scoop (bucket propio o PR a ScoopInstaller/Extras).

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
| **8** | Extras: caché de búsquedas en memoria (`[cache]`, decorador `CachingProvider`), modo radio (tecla `r`, sub-modos con `R`: artista, similares vía mix RD de YouTube, random desde favoritos, random género, rarezas) y metadatos de publicación en crates.io — **completada** (la publicación en crates.io/Homebrew/scoop queda como paso manual del autor) |
| **9** | Prioridades de reproducción y UX: seek dentro de la pista (`←`/`→`, `seek` en el trait `Player`), modos de repetición (off → all → one), shuffle de la cola, overlay de ayuda contextual con `?` — **completada** |
| 9.1 | Bug (panic): `cannot convert float seconds to Duration: value is negative` (`core/src/time.rs`). mpv puede reportar `time-pos` negativo (buffering inicial o seek antes del inicio) y `Duration::from_secs_f64` panicua con negativos en `src/player/mpv.rs` (`status()`, helper `secs_to_duration`); también clamado en `src/providers/ytdlp.rs` (`parse_entry`, duración inválida). Viola RNF-4 (ningún error produce panic) — **completada** |
| 9.2 | Prefetch de radio: la expansión se dispara al empezar a sonar la última pista de la cola (antes: al agotarse), encolando la tanda siguiente en segundo plano sin interrumpir; al terminar la pista la siguiente entra sin pausa. Guardas: `radio_expanded_for` (no duplicar búsquedas ni martillear yt-dlp tras fallos) y `playback_active` (RadioDone solo auto-reproduce con el reproductor parado) — **completada** |
| **10** | Navegación en listas largas: `PageUp`/`PageDown` y `g`/`G` en Resultados, Cola, Favoritos y playlists — **completada** |
| 11 | Descubrimiento: autocompletado de la búsqueda a partir del historial local (`→` acepta el ghost text); overlay de letras (API lrclib.net, tecla `L`); panel "sonando ahora" con metadatos ampliados y carátula de YouTube (kitty/sixel/halfblocks vía `ratatui-image`, tecla `i`) — **completada** |
| 12 | Integración con el sistema: notificaciones de escritorio al cambiar de pista (`notify-rust`, `[integrations] notifications`); MPRIS en Linux (`souvlaki` con backend zbus: teclas multimedia, widgets, KDE Connect; `[integrations] media_controls`). Media keys en Windows/macOS **descartadas**: requieren ventana (HWND) o event loop de GUI (AppDelegate), incompatibles con una TUI pura — **completada** |
| 13 | Datos y portabilidad: exportar/importar playlists y favoritos (JSON/M3U); estadísticas locales de escucha (conteos, artistas más oídos, vista "tus más escuchadas") — **completada** (teclas `t`/`X`/`I`; `src/stats.rs`, `src/portable.rs`, overlay de estadísticas) |
| **14** | Pulido: cola persistente entre sesiones (`session.json`: restaurar cola + pista + posición al arrancar, con reanudación vía `start` de mpv); temas de color configurables en `[theme]` de `config.toml`; título de la pestaña del terminal con la pista en reproducción (OSC 2) — **completada** |
| **15** | Limpieza de la cola: con el modo radio la cola crece sin límite (~5 pistas por tanda). Al superar 500 pistas (`MAX_QUEUE` en `src/queue.rs`) se podan las ya reproducidas antes de encolar más, conservando las 20 anteriores a la actual (`KEEP_BEFORE_CURRENT`) para la navegación `p`; sin reproducción en curso o con la actual cerca del inicio no se poda — **completada** |
| **16** | Robustez anti-bot de reproducción (tras evaluar y descartar la YouTube Data API v3: no da URLs de stream y exige API key por usuario): rotación de `player_client` × cookies con caché de la combinación ganadora y reintento del plan completo si caduca; detección del plugin bgutil PO Token (`[youtube] pot_provider`); aviso al arrancar si yt-dlp tiene más de 90 días — **completada** |
| **17** | Robustez operativa: auto-respawn de mpv si muere; epoch de búsqueda (resultados rezagados no sobrescriben); reintento con backoff en yt-dlp; sesión y estadísticas guardadas periódicamente (resistencia a kill -9); SaveLock anti-stale (huérfanos por PID muerto o edad); normalización offline de artistas en estadísticas (MusicBrainz evaluado y descartado: rate limit 1 req/s en el path crítico) — **completada** |

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
