```
 ___ _        _ _ __  __         _
/ __| |_  ___| | |  \/  |_  _ __(_)__
\__ \ ' \/ -_) | | |\/| | || (_-< / _|
|___/_||_\___|_|_|_|  |_|\_,_/__/_\__|
```

**Toda tu música, sin salir de la terminal.**

[![CI](https://github.com/devFuentesQuijon/ShellMusic/actions/workflows/ci.yml/badge.svg)](https://github.com/devFuentesQuijon/ShellMusic/actions/workflows/ci.yml)
[![Licencia: MIT](https://img.shields.io/badge/licencia-MIT-blue.svg)](LICENSE)

TUI ultraliviana para buscar y reproducir música de YouTube Music y SoundCloud. Escrita en Rust con ratatui; la reproducción corre a cargo de **mpv** (controlado por JSON IPC) y la búsqueda/resolución de **yt-dlp**.

![Demo de ShellMusic](docs/demo.gif)

## Características

- **Busca y reproduce** YouTube Music y SoundCloud sin abrir el navegador (`m` cambia de proveedor en caliente).
- **Cola gestionable**: añade, quita, reordena (`J`/`K`), baraja (`S`) y recorre listas largas por página (`PgUp`/`PgDn`) o al salto (`g`/`G`).
- **Cola persistente**: al cerrar y reabrir, la cola, la pista y la posición se reanudan donde lo dejaste.
- **Temas de color** configurables en `config.toml` (nombres, `#rrggbb` o índices de paleta).
- **Título de la pestaña** con la pista en reproducción (`♪ artista — título · ShellMusic`).
- **Modo radio** con cinco sub-modos (artista, similares, random, random género, rarezas): la siguiente tanda se busca mientras suena la última pista y entra sin pausa.
- **Repetición** off / toda la cola / pista actual (`l`) y **seek** de ±5 s o ±30 s (`←`/`→`).
- **Favoritos y playlists locales** con escritura atómica y recuperación ante archivos corruptos: cero cuentas, cero nube.
- **Historial de búsquedas** navegable con `↑`/`↓`, como en un shell.
- **Estadísticas de escucha** (`t`): tus pistas y artistas más escuchados, con reproducción directa desde el ranking.
- **Portabilidad** (`X`/`I`): exporta playlists y favoritos a JSON o M3U e impórtalos en otro equipo.
- **Integración con el sistema**: notificación de escritorio al cambiar de pista y, en Linux, MPRIS (teclas multimedia, widgets del escritorio, KDE Connect).
- **Ayuda contextual** de atajos con `?`.
- **Caché en memoria** de búsquedas y mitigación del bloqueo anti-bot de YouTube (cookies del navegador solo cuando hacen falta; nunca salen de tu equipo).
- Binario único para Linux, macOS y Windows.

![ShellMusic reproduciendo](docs/screenshot.png)

## Instalación

### Opción 1: binario precompilado

Descarga el binario para tu SO desde [GitHub Releases](https://github.com/devFuentesQuijon/ShellMusic/releases) (`smusic-linux-x86_64`, `smusic-macos-aarch64`, `smusic-windows-x86_64.exe`), dale permiso de ejecución (`chmod +x`) y ponlo en tu `PATH`.

### Opción 2: compilar

```sh
# instalación como comando del sistema (~/.cargo/bin debe estar en el PATH)
cargo install --path .
smusic

# o durante desarrollo, sin instalar
cargo run --release
```

## Dependencias

ShellMusic necesita `mpv` y `yt-dlp` en el `PATH`. Si falta alguno, muestra las instrucciones y sale sin panic.

| SO | Instalación |
|---|---|
| Linux | `sudo apt install mpv` (o tu gestor de paquetes) + `pip install yt-dlp` |
| macOS | `brew install mpv yt-dlp` |
| Windows | `winget install mpv yt-dlp` |

## Atajos de teclado

| Tecla | Contexto | Acción |
|---|---|---|
| Texto + Enter | Búsqueda | Ejecutar búsqueda |
| ↑ / ↓ | Búsqueda | Navegar el historial de búsquedas recientes (al bajar de la más reciente se restaura lo que estabas escribiendo) |
| Esc / Tab | Búsqueda | Pasar a resultados |
| Tab / Shift+Tab | Resultados / Cola / Favoritos | Ciclar foco: Búsqueda → Resultados → Cola → Favoritos |
| ↑ / ↓ (o k / j) | Resultados / Cola / Favoritos | Mover selección |
| Enter | Resultados | Reproducir selección (la añade a la cola si no está) |
| a | Resultados | Añadir a la cola sin interrumpir la reproducción |
| f | Resultados / Cola | Marcar / quitar favorito (o de la pista que suena si no hay selección) |
| Enter | Cola | Reproducir la pista seleccionada |
| d | Cola | Eliminar de la cola |
| J / K | Cola | Mover la pista abajo / arriba |
| Enter | Favoritos | Reproducir (y añadir a la cola) |
| a | Favoritos | Añadir a la cola sin interrumpir |
| d / f | Favoritos | Quitar de favoritos |
| Espacio | Resultados / Cola / Favoritos | Play / pausa |
| s | Resultados / Cola / Favoritos | Stop |
| ← / → | Resultados / Cola / Favoritos | Seek ∓5 s dentro de la pista (con Shift: ∓30 s) |
| + / - | Resultados / Cola / Favoritos | Volumen arriba / abajo |
| n / p | Resultados / Cola / Favoritos | Siguiente / anterior pista de la cola (sin bucle) |
| l | Resultados / Cola / Favoritos | Ciclar repetición: off → toda la cola → pista actual |
| S | Resultados / Cola / Favoritos | Barajar la cola (la pista que suena mantiene su lugar) |
| ? | Resultados / Cola / Favoritos | Ayuda de atajos (overlay; se cierra con Esc o ?) |
| PgUp / PgDn | Listas (resultados, cola, favoritos, playlists) | Saltar una página |
| g / G | Listas (resultados, cola, favoritos, playlists) | Ir al inicio / al final |
| / | Resultados / Cola / Favoritos | Foco a búsqueda |
| q | Resultados / Cola / Favoritos | Salir |
| P | Resultados / Cola / Favoritos | Abrir gestor de playlists (overlay) |
| A | Resultados / Cola / Favoritos | Añadir la pista seleccionada (o la que suena) a una playlist |
| m | Resultados / Cola / Favoritos | Cambiar de proveedor de búsqueda (YouTube / SoundCloud) |
| t | Resultados / Cola / Favoritos | Estadísticas de escucha (overlay; Tab alterna pistas/artistas) |
| X | Resultados / Cola / Favoritos | Exportar playlists/favoritos a JSON (en una playlist: a M3U) |
| I | Resultados / Cola / Favoritos | Importar desde `.json` o `.m3u` |
| r | Resultados / Cola / Favoritos | Activar / desactivar el modo radio |
| R | Resultados / Cola / Favoritos | Ciclar el sub-modo de radio: artista → similares → random → random género → rarezas |
| Ctrl+C | Global | Salir |

Con el overlay de playlists abierto (modal; captura todas las teclas salvo Ctrl+C):

| Tecla | Vista | Acción |
|---|---|---|
| ↑ / ↓ (o k / j) | Todas | Mover selección |
| n | Lista | Nueva playlist (pide nombre) |
| d | Lista | Eliminar playlist (pulsa `d` dos veces para confirmar) |
| Enter | Lista | Abrir contenido |
| Enter | Contenido | Reproducir la pista (la añade a la cola) |
| a | Contenido | Añadir la pista a la cola sin interrumpir |
| A | Contenido | Reproducir la playlist entera (encola todo y suena la primera) |
| d | Contenido | Quitar la pista de la playlist |
| J / K | Contenido | Reordenar la pista |
| Esc | Lista / Contenido / Selector | Cerrar / volver / cancelar |

## Playlists

Playlists locales gestionadas desde el overlay modal (`P`): crear (`n`), eliminar (`d` dos veces), abrir (`Enter`) y, dentro, reordenar (`J`/`K`), quitar pistas (`d`) o reproducir todo (`A`). Con `A` sobre cualquier pista la añades a una playlist existente o a una nueva (sin duplicados por pista). Los nombres de playlist son únicos.

Se guardan en `playlists.json` junto a `favorites.json` (`~/.local/share/shellmusic/playlists.json` en Linux, `~/Library/Application Support/shellmusic/playlists.json` en macOS, `%APPDATA%\shellmusic\playlists.json` en Windows), con escritura atómica y recuperación ante archivo corrupto (`.bak`).

Mientras se edita la búsqueda, las teclas se capturan como texto; los controles de reproducción aplican con el foco en Resultados, Cola o Favoritos.

## Favoritos

`f` marca o desmarca la pista seleccionada (o la que suena) como favorita; las favoritas llevan marcador `★` en todos los paneles. El panel Favoritos (cuarta vista del ciclo `Tab`) permite reproducirlas (`Enter`), encolarlas (`a`) o quitarlas (`d`).

Se guardan en `favorites.json` en el directorio de datos nativo (`~/.local/share/shellmusic/favorites.json` en Linux, `~/Library/Application Support/shellmusic/favorites.json` en macOS, `%APPDATA%\shellmusic\favorites.json` en Windows), con escritura atómica. Si el archivo está corrupto se aparta a `favorites.json.bak` y se empieza con lista vacía, sin cerrar la app. Todo es local: no hay cuentas ni sincronización.

## Cola de reproducción

El panel derecho muestra la cola: posición, título, artista y duración, con `♪` en la pista que suena y `▶` en la seleccionada. La barra del reproductor muestra la posición en la cola (`cola 3/12`).

- Al terminar una pista de forma natural, avanza automáticamente a la siguiente de la cola (autoplay). `s` (stop) no dispara autoplay.
- Si eliminas con `d` la pista que está sonando, se reproduce la que ocupa su lugar; si era la última, se detiene.
- La cola es persistente: se guarda en `session.json` junto a `favorites.json` al cambiar de pista y al salir, y al arrancar se restaura la cola y se reanuda la pista desde la posición en la que estaba.

## Título de la pestaña y temas

La pestaña del terminal muestra «ShellMusic» en reposo y «♪ artista — título · ShellMusic» mientras suena una pista; al salir se restablece el título habitual del terminal.

Los colores de la interfaz se configuran en la sección `[theme]` de `config.toml`:

```toml
[theme]
accent = "cyan"        # bordes del panel enfocado y títulos de sección
highlight = "yellow"   # mensajes informativos y teclas en la ayuda
error = "red"          # mensajes de error
muted = "darkgray"     # línea de hints y texto secundario
progress = "green"     # barra de progreso
```

Acepta nombres (`cyan`, `lightred`, `darkgray`…), hex (`#rrggbb`) e índices de paleta (0-255); un valor inválido usa el color por defecto y avisa en la línea de estado.

## Repetición y orden aleatorio

`l` cicla el modo de repetición (la barra del reproductor muestra `rep:all` o `rep:1`):

- **off** (por defecto): al acabar la cola se detiene, o entra el modo radio si está activo.
- **toda la cola**: al terminar la última pista, la cola vuelve a empezar por la primera.
- **pista actual**: repite en bucle la pista que suena.

`S` baraja el orden de la cola; la pista en reproducción conserva su posición. La repetición actúa solo al terminar la pista de forma natural: `n`/`p` manuales siguen sin hacer bucle.

## Configuración

En el primer arranque se crea `config.toml` en la ruta nativa de configuración (`~/.config/shellmusic/config.toml` en Linux, `~/Library/Application Support/shellmusic/config.toml` en macOS, `%APPDATA%\shellmusic\config.toml` en Windows):

```toml
[general]
results_per_search = 15   # 1-50
providers = ["youtube", "soundcloud"]   # proveedores habilitados; la tecla m cicla entre ellos
default_provider = "youtube"            # proveedor activo al arrancar (debe estar en la lista)

[audio]
initial_volume = 70       # 0-100

[youtube]
cookies_from_browser = "auto"   # "auto" | "firefox"/"chrome"/... | "" (nunca usar cookies)
player_client = "android"          # cliente de YouTube que emula yt-dlp
remote_components = "ejs:github"   # solucionador de retos JS de yt-dlp ("" = desactivado)

[cache]
enabled = true      # caché en memoria de búsquedas (solo dura la sesión)
ttl_minutes = 15    # 1-120

[radio]
enabled = false     # modo radio activo al arrancar (se alterna con la tecla r)

[theme]
accent = "cyan"        # colores de la interfaz; ver «Título de la pestaña y temas»

[integrations]
notifications = true    # notificación de escritorio al cambiar de pista
media_controls = true   # MPRIS en Linux; ver «Integración con el sistema»
```

Los proveedores se buscan con yt-dlp (`ytsearch` para YouTube, `scsearch` para SoundCloud); el proveedor activo se muestra en el título de la caja de búsqueda y se cambia en caliente con `m`. Nombres desconocidos en `providers` se ignoran; si la lista queda vacía se usa `["youtube"]`, y si `default_provider` no está en la lista se usa el primero.

## Caché de búsquedas

Las búsquedas se cachean en memoria durante la sesión (sección `[cache]`): repetir la misma búsqueda en el mismo proveedor no vuelve a invocar yt-dlp hasta que expira el TTL. No se guarda nada en disco.

## Historial de búsquedas

Con la caja de búsqueda enfocada, `↑`/`↓` recorren las búsquedas recientes (como el historial de un shell): `↑` va hacia las más antiguas y, al bajar con `↓` más allá de la más reciente, se restaura lo que estabas escribiendo. Se guardan las últimas 50 en `history.json` junto a `favorites.json`, con la misma escritura atómica.

## Modo radio

Con `r` activas el modo radio (estado inicial configurable en `[radio] enabled`; la barra del reproductor muestra `radio:ON·<modo>`). Cuando empieza a sonar la última pista de la cola, ShellMusic busca en segundo plano y encola hasta 5 pistas nuevas según el sub-modo activo (que se cicla con `R`), de modo que al terminar la pista la siguiente ya está en cola y entra sin pausa:

- **artista**: pistas del último artista sonado.
- **similares**: el mix autogenerado de YouTube para la última pista (la radio `RD` real de YouTube; en SoundCloud, fallback a búsqueda por artista + título).
- **random**: un artista al azar de tus favoritos (sin favoritos, cae a random género).
- **random género**: una palabra clave de género/década al azar ("80s rock", "bossa nova", …).
- **rarezas**: pistas menos conocidas del último artista (descarta los primeros hits y baraja el resto).

En todos los modos se excluyen las pistas ya reproducidas o encoladas, y no entran dos versiones de la misma canción (dedupe por título normalizado). Si no hay nada nuevo, la reproducción se detiene al agotarse la cola con un aviso. El modo inicial se configura con `[radio] mode`.

## Portabilidad y estadísticas

**Exportar** (`X` desde Resultados/Cola/Favoritos): menú para volcar playlists + favoritos, solo playlists o solo favoritos a un JSON (la ruta admite `~`). En la vista de contenido de una playlist (`P` → `Enter`), `X` la exporta a **M3U extendido** (`#EXTINF` con duración y «artista - título»).

**Importar** (`I`): por extensión — `.json` fusiona sin duplicados (favoritos por pista, playlists por nombre con merge de pistas), `.m3u`/`.m3u8` crea una playlist con el nombre del archivo; las URLs de YouTube (`watch?v=` / `youtu.be/`) recuperan su id.

**Estadísticas** (`t`): cada pista que arranca cuenta una reproducción en `stats.json` (mismo directorio de datos, escritura atómica). El overlay «Tus más escuchadas» muestra el ranking de pistas (`Enter` reproduce, `a` encola) y, con `Tab`, el de artistas.

## Integración con el sistema

Al cambiar de pista, ShellMusic muestra una **notificación de escritorio** con título y artista (Linux, macOS y Windows). En **Linux** además se registra como reproductor **MPRIS** (`org.mpris.MediaPlayer2.shellmusic`): las teclas multimedia del teclado, los widgets del escritorio (GNOME/KDE) y **KDE Connect** pueden pausar, parar, cambiar de pista, hacer seek y ajustar el volumen; compruébalo con `playerctl -l` y `playerctl play-pause`.

Se desactivan por separado en `config.toml`:

```toml
[integrations]
notifications = true    # notificación de escritorio al cambiar de pista
media_controls = true   # MPRIS en Linux (sin efecto en macOS/Windows)
```

En una sesión sin D-Bus (SSH, headless) los media controls se desactivan solos con un aviso en la línea de estado; la app sigue funcionando. En macOS/Windows no hay media controls porque exigen una ventana o un event loop de GUI, incompatibles con una TUI pura.

## Bloqueo anti-bot de YouTube

YouTube bloquea la resolución de streams desde muchas IPs (HTTP 429 "Sign in to confirm you're not a bot" o HTTP 403 al abrir el stream). La búsqueda suele funcionar, pero la reproducción falla. La sección `[youtube]` lo mitiga:

- `cookies_from_browser`:
  - `"auto"` (por defecto): reproduce primero **sin cookies**; solo si YouTube bloquea el stream, reintenta con las cookies de cada navegador instalado detectado (firefox, chrome, chromium, brave, edge, opera, vivaldi — firefox primero). El navegador que funciona se recuerda durante la sesión, así que las siguientes pistas no repiten el ciclo. Si todo falla, la UI te pide iniciar sesión en YouTube desde algún navegador instalado (basta una sesión de visitante, no hace falta cuenta).
  - `"firefox"`, `"chrome"`, ...: usa solo ese navegador, sin auto-detección. Requiere haber iniciado sesión de YouTube en él.
  - `""`: nunca usa cookies.
- `player_client`: el cliente `android` evita la exigencia de GVS PO Token que produce el 403 en los clientes web por defecto.
- `remote_components`: permite a yt-dlp descargar el solucionador de retos JS de firma desde GitHub; necesita un runtime JS instalado (`bun`, `deno` o `node`).

**Privacidad**: en modo `auto` la app solo lee las cookies del navegador cuando el intento sin cookies falla. Las cookies nunca salen de tu equipo: se pasan únicamente al proceso local de yt-dlp/mpv para autenticar la petición a YouTube.

Si la reproducción falla, el mensaje de error en la barra de estado incluye el motivo real reportado por mpv/yt-dlp.

## Tests de integración

Requieren `mpv`, `yt-dlp` y acceso a red:

```sh
cargo test -- --ignored
```
