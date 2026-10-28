# Configuración y temas

En el primer arranque se crea `config.toml` en la ruta nativa de configuración (`~/.config/shellmusic/config.toml` en Linux, `~/Library/Application Support/shellmusic/config.toml` en macOS, `%APPDATA%\shellmusic\config.toml` en Windows). Las claves ausentes toman su valor por defecto (compatibilidad hacia atrás).

```toml
[general]
results_per_search = 15   # 1-50
providers = ["youtube", "soundcloud", "bandcamp"]   # proveedores habilitados; la tecla m cicla entre ellos
default_provider = "youtube"            # proveedor activo al arrancar (debe estar en la lista)
mouse = true                # ratón: click en la barra = seek · rueda = volumen · click en panel = foco (false deja la selección de texto del terminal)

[audio]
initial_volume = 70       # 0-100
format = ""               # selector de formato de yt-dlp (mpv --ytdl-format): "bestaudio", "worst", "bestaudio[abr<=128]", ...; "" = criterio por defecto de yt-dlp
gapless = false           # true: reproducción sin pausa entre pistas consecutivas de la cola (mpv apila la siguiente por adelantado: extrae su URL antes)

[youtube]
cookies_from_browser = "auto"   # "auto" | "firefox"/"chrome"/... | "" (nunca usar cookies)
player_client = "android"          # cliente de YouTube preferido; si bloquea, rota automáticamente
pot_provider = "auto"              # usar el plugin bgutil PO Token de yt-dlp si está instalado
remote_components = "ejs:github"   # solucionador de retos JS de yt-dlp ("" = desactivado)

[cache]
enabled = true      # caché en memoria de búsquedas (solo dura la sesión)
ttl_minutes = 15    # 1-120

[radio]
enabled = false     # modo radio activo al arrancar (se alterna con la tecla r)

[theme]
accent = "cyan"        # bordes del panel enfocado y títulos de sección
highlight = "yellow"   # mensajes informativos y teclas en la ayuda
error = "red"          # mensajes de error
muted = "darkgray"     # línea de hints y texto secundario
progress = "green"     # barra de progreso

[integrations]
notifications = true    # notificación de escritorio al cambiar de pista (con carátula en Linux)
media_controls = true   # MPRIS en Linux; ver «Integración con el sistema»
discord = false         # presencia en Discord («escuchando: …»); requiere el cliente Discord abierto
discord_client_id = ""  # override para forks; vacío = el ID embebido en el binario

[lastfm]
enabled = false         # scrobbling de escucha a Last.fm
api_key = ""             # créalas en https://www.last.fm/api/account/create
secret = ""              # y autoriza con: smusic --lastfm-auth

[mcp]
enabled = false         # socket de control local para agentes de IA (cliente de referencia: smusic-mcp)
```

- Los proveedores se buscan con yt-dlp (`ytsearch` para YouTube, `scsearch` para SoundCloud); el proveedor activo se muestra en el título de la caja de búsqueda y se cambia en caliente con `m`. Nombres desconocidos en `providers` se ignoran; si la lista queda vacía se usa `["youtube"]`, y si `default_provider` no está en la lista se usa el primero.
- `[general] mouse = false` desactiva la captura de ratón si prefieres la selección de texto del terminal; con el ratón activo, click en la barra del reproductor = seek, rueda = volumen y click en panel = foco/selección.
- Pegar una URL en la búsqueda (YouTube `watch`/`youtu.be`/playlist/shorts, SoundCloud pista o `on.soundcloud.com`, Bandcamp pista/álbum/página de artista) la resuelve yt-dlp directamente contra esa URL — una pista o los primeros `results_per_search` de una playlist/álbum — con el proveedor de la URL aunque no sea el activo. Cualquier otro texto se busca literal. Bandcamp no admite búsqueda de texto (yt-dlp no la implementa): el intento responde con una indicación de usar una URL.
- `[audio] format` se pasa a mpv como `--ytdl-format`: elige el formato de extracción (p. ej. `bestaudio` para ahorrar ancho de banda, `worst` en conexiones malas); vacío mantiene el criterio por defecto de yt-dlp.
- `[audio] gapless = true` reproduce la cola sin pausa entre pistas: al arrancar una pista, la siguiente se apila en mpv (`loadfile append` + `--gapless-audio`), que extrae su URL por adelantado. Si la cola cambia entre medias (borrado, movido, barajado), la transición se recarga con el comportamiento normal. El crossfade no está disponible: mpv no ofrece un filtro de crossfade usable entre pistas cargadas una a una.
- Los colores aceptan nombres (`cyan`, `lightred`, `darkgray`…), hex (`#rrggbb`) e índices de paleta (0-255); un valor inválido usa el color por defecto y avisa en la línea de estado.
- Hay una [colección de temas listos para copiar](temas.md) (incluida una temática de monstruos clásicos para Halloween).
- El detalle de `[youtube]` (anti-bot) está en [Bloqueo anti-bot de YouTube](anti-bot.md).
- `[integrations] discord = true` activa la presencia en Discord con el cliente abierto; los usuarios no necesitan crear nada (el Application ID va embebido en el binario). El scrobbling Last.fm (`[lastfm]`) se da de alta con `smusic --lastfm-auth`; detalle y reglas en [Integración con el sistema](integracion-sistema.md).

## Temas

La pestaña del terminal muestra «ShellMusic» en reposo y «♪ artista — título · ShellMusic» mientras suena una pista; al salir se restablece el título habitual del terminal.

Los colores de la interfaz se configuran en `[theme]` (tabla de arriba). Cambios recientes de tema se aplican al reiniciar la app.

## Caché de búsquedas

Las búsquedas se cachean en memoria durante la sesión (sección `[cache]`): repetir la misma búsqueda en el mismo proveedor no vuelve a invocar yt-dlp hasta que expira el TTL. No se guarda nada en disco.

## Historial de búsquedas

Con la caja de búsqueda enfocada, `↑`/`↓` recorren las búsquedas recientes (como el historial de un shell): `↑` va hacia las más antiguas y, al bajar con `↓` más allá de la más reciente, se restaura lo que estabas escribiendo. Se guardan las últimas 50 en `history.json` junto a `favorites.json`, con la misma escritura atómica.
