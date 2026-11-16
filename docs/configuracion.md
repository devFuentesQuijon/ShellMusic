# Configuración y temas

En el primer arranque se crea `config.toml` en la ruta nativa de configuración (`~/.config/shellmusic/config.toml` en Linux, `~/Library/Application Support/shellmusic/config.toml` en macOS, `%APPDATA%\shellmusic\config.toml` en Windows). Las claves ausentes toman su valor por defecto (compatibilidad hacia atrás).

```toml
[general]
results_per_search = 15   # 1-50
providers = ["youtube", "soundcloud", "bandcamp"]   # proveedores habilitados; la tecla m cicla entre ellos
default_provider = "youtube"            # proveedor activo al arrancar (debe estar en la lista)
mouse = true                # ratón: click en la barra = seek · rueda = volumen · click en panel = foco (false deja la selección de texto del terminal)

[ui]
style = "classic"    # classic | modern — modern: bordes redondeados y barra de progreso con degradado; se cambia en caliente con «: ui» o el menú F2

[audio]
initial_volume = 70       # 0-100
format = ""               # selector de formato de yt-dlp (mpv --ytdl-format): "bestaudio", "worst", "bestaudio[abr<=128]", ...; "" = criterio por defecto de yt-dlp
gapless = false           # true: reproducción sin pausa entre pistas consecutivas de la cola (mpv apila la siguiente por adelantado: extrae su URL antes)
crossfade = 0             # 0 = off · 1-10: fundido de salida en los últimos N s y de entrada en los primeros N s de cada pista (filtro afade de mpv); combínalo con gapless

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

[sponsorblock]
enabled = false         # saltar segmentos (sponsor/intro/outro) via sponsor.ajay.app; AVISO: envía el ID de cada vídeo reproducido a ese servidor
categories = ["sponsor", "intro", "outro"]   # también: interaction, selfpromo, preview, music_offtopic, filler

[ytm]
enabled = false         # YouTube Music con tu cuenta (pip install ytmusicapi y alta con: smusic --ytm-auth [--from-browser])
like_writeback = true   # la tecla f también da/quita like en YouTube Music
```

- Los proveedores se buscan con yt-dlp (`ytsearch` para YouTube, `scsearch` para SoundCloud); el proveedor activo se muestra en el título de la caja de búsqueda y se cambia en caliente con `m`. Nombres desconocidos en `providers` se ignoran; si la lista queda vacía se usa `["youtube"]`, y si `default_provider` no está en la lista se usa el primero.
- `[general] mouse = false` desactiva la captura de ratón si prefieres la selección de texto del terminal; con el ratón activo, click en la barra del reproductor = seek, rueda = volumen y click en panel = foco/selección.
- Pegar una URL en la búsqueda (YouTube `watch`/`youtu.be`/playlist/shorts, SoundCloud pista o `on.soundcloud.com`, Bandcamp pista/álbum/página de artista) la resuelve yt-dlp directamente contra esa URL — una pista o los primeros `results_per_search` de una playlist/álbum — con el proveedor de la URL aunque no sea el activo. Cualquier otro texto se busca literal. Bandcamp no admite búsqueda de texto (yt-dlp no la implementa): el intento responde con una indicación de usar una URL.
- `[audio] format` se pasa a mpv como `--ytdl-format`: elige el formato de extracción (p. ej. `bestaudio` para ahorrar ancho de banda, `worst` en conexiones malas); vacío mantiene el criterio por defecto de yt-dlp.
- `[audio] gapless = true` reproduce la cola sin pausa entre pistas: al arrancar una pista, la siguiente se apila en mpv (`loadfile append` + `--gapless-audio`), que extrae su URL por adelantado. Si la cola cambia entre medias (borrado, movido, barajado), la transición se recarga con el comportamiento normal. Con `[audio] crossfade = 1-10` además cada pista funde su salida en los últimos N segundos y su entrada en los primeros N (filtro `afade` de FFmpeg en la cadena de audio de mpv), ajustable en caliente en el menú F2 (Reproducción → Crossfade). Es un fundido por volumen, no un solapamiento real (mezclar ambas pistas exigiría `lavfi-complex` con dos fuentes: descartado por fragilidad); combinado con gapless, el resultado auditivo es el de un crossfade continuo. Las pistas sin duración conocida solo reciben fade-in.
- Los colores aceptan nombres (`cyan`, `lightred`, `darkgray`…), hex (`#rrggbb`) e índices de paleta (0-255); un valor inválido usa el color por defecto y avisa en la línea de estado.
- Hay una [colección de temas listos para copiar](temas.md) (incluida una temática de monstruos clásicos para Halloween).
- El detalle de `[youtube]` (anti-bot) está en [Bloqueo anti-bot de YouTube](anti-bot.md).
- `[integrations] discord = true` activa la presencia en Discord con el cliente abierto; los usuarios no necesitan crear nada (el Application ID va embebido en el binario). El scrobbling Last.fm (`[lastfm]`) se da de alta con `smusic --lastfm-auth`; detalle y reglas en [Integración con el sistema](integracion-sistema.md).
- `[sponsorblock] enabled = true` salta automáticamente los segmentos que la comunidad marca en los vídeos (sponsor, intro, outro…); la lista de categorías es configurable (`music_offtopic` corta el parlamento de los sets de DJ). **Privacidad**: cada reproducción de YouTube envía el ID del vídeo a sponsor.ajay.app.
- `[ytm]` conecta tu cuenta de YouTube Music (recomendaciones, likes, playlists): alta con `pip install ytmusicapi` + `smusic --ytm-auth --from-browser` (lee las cookies del navegador; sin DevTools) o `smusic --ytm-auth` (pega los headers) + `enabled = true`. Comandos `: ytm likes|import <nombre>|home|radio` y like write-back con la tecla `f`; detalle en [Atajos de teclado](atajos.md).

## Temas

La pestaña del terminal muestra «ShellMusic» en reposo y «♪ artista — título · ShellMusic» mientras suena una pista; al salir se restablece el título habitual del terminal.

Los colores de la interfaz se configuran en `[theme]` (tabla de arriba) y se cambian en caliente con `: theme <nombre>` (aplica y guarda) o el menú F2. Los presets empaquetados y los temas propios de `~/.config/shellmusic/themes/*.toml` se listan en [Temas](temas.md). El estilo de interfaz (`[ui] style`, classic/modern) también es conmutable en caliente (`: ui`).

## Caché de búsquedas

Las búsquedas se cachean en memoria durante la sesión (sección `[cache]`): repetir la misma búsqueda en el mismo proveedor no vuelve a invocar yt-dlp hasta que expira el TTL. No se guarda nada en disco.

## Historial de búsquedas

Con la caja de búsqueda enfocada, `↑`/`↓` recorren las búsquedas recientes (como el historial de un shell): `↑` va hacia las más antiguas y, al bajar con `↓` más allá de la más reciente, se restaura lo que estabas escribiendo. Se guardan las últimas 50 en `history.json` junto a `favorites.json`, con la misma escritura atómica.
