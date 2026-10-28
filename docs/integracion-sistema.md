# Integración con el sistema

Al cambiar de pista, ShellMusic muestra una **notificación de escritorio** con título, artista y —cuando está disponible— la **carátula** de la pista (Linux; en macOS/Windows la notificación es de texto, los sistemas no renderizan la imagen). En **Linux** además se registra como reproductor **MPRIS** (`org.mpris.MediaPlayer2.shellmusic`): las teclas multimedia del teclado, los widgets del escritorio (GNOME/KDE) y **KDE Connect** pueden pausar, parar, cambiar de pista, hacer seek y ajustar el volumen; compruébalo con `playerctl -l` y `playerctl play-pause`.

Matriz de funcionalidades por SO:

| Funcionalidad | Linux | macOS | Windows |
|---|:-:|:-:|:-:|
| Búsqueda y reproducción | ✅ | ✅ | ✅ |
| Cola persistente, favoritos, playlists, estadísticas | ✅ | ✅ | ✅ |
| Notificaciones de escritorio | ✅ (con carátula) | ✅ (texto) | ✅ (texto) |
| Media controls (teclas multimedia, widgets, KDE Connect) | ✅ (MPRIS) | — | — |
| Carátula en el panel «sonando ahora» | kitty / sixel | iTerm2 | halfblocks |
| Discord Rich Presence | ✅ (socket Unix) | ✅ (socket Unix) | pendiente (named pipes) |
| Scrobbling Last.fm | ✅ | ✅ | ✅ |

La reproducción local está verificada en CI en los tres SO (el test IPC de mpv corre en Linux, macOS y Windows). En macOS/Windows no hay media controls porque exigen una ventana o un event loop de GUI, incompatibles con una TUI pura. En una sesión sin D-Bus (SSH, headless) los media controls se desactivan solos con un aviso en la línea de estado; la app sigue funcionando.

## Discord Rich Presence

Con `[integrations] discord = true` y el **cliente de Discord abierto**, tu perfil muestra «ShellMusic — reproduciendo» con título — artista y temporizador (o «en pausa» / sin estado).

- El protocolo IPC es propio (`src/discord.rs`): socket `discord-ipc-N` local, JSON con framing de 8 bytes, reconexión silenciosa cada ~15 s si Discord no está o se reinicia. Sin dependencias nuevas.
- El **Application ID es por aplicación, no por usuario**: el autor lo registra una vez en [discord.com/developers](https://discord.com/developers/applications) (New Application → Application ID) y queda embebido en el binario. **Los usuarios no crean nada.** Los forks pueden poner el suyo en `[integrations] discord_client_id`.
- Si falta el ID y `discord = true`, un aviso único al arrancar lo indica (la app funciona igual).
- Verificación manual: con Discord abierto, reproduce una pista y mira tu perfil.

## Scrobbling Last.fm

Opt-in con `[lastfm]`:

```toml
[lastfm]
enabled = true
api_key = "…"   # https://www.last.fm/api/account/create (gratis, minutos)
secret = "…"
```

1. Crea la API key y secret en la página de cuenta de Last.fm (misma fricción que documentó el SPEC para esta fase: sin revisión, a diferencia del Development Mode de Spotify).
2. Ejecuta **`smusic --lastfm-auth`**: imprime una URL, la abres en el navegador, autorizas ShellMusic, vuelves y pulsas Enter. La sesión queda guardada en `lastfm.json` del directorio de datos.
3. A partir de ahí, cada pista oída se scrobbla sola.

Reglas (las oficiales de Last.fm): la pista debe durar **más de 30 s** y haberse oído **`min(30 s, mitad de la duración)`** o más — medido con la posición máxima alcanzada, así que hacer seek atrás no infla el conteo. Se scrobbla al cambiar de pista, con stop y al cerrar la app; mientras suena, Last.fm recibe el «now playing». Si la sesión caduca, la app avisa pidiendo re-autenticar con `smusic --lastfm-auth`; los errores de red pasajeros son silenciosos.

## Carátula en las notificaciones

Al cambiar de pista se descarga la carátula de YouTube (la misma que usa el panel «sonando ahora»), se guarda como PNG en el directorio de datos y se adjunta a la notificación con `image-path` de D-Bus. Si la descarga tarda, la notificación espera (~200-400 ms típicos) y sale con la imagen; si falla, sale de texto. No hay configuración extra: sigue el interruptor `notifications`.

## Configuración

Se desactivan por separado en `config.toml`:

```toml
[integrations]
notifications = true    # notificación de escritorio al cambiar de pista
media_controls = true   # MPRIS en Linux (sin efecto en macOS/Windows)
discord = false         # presencia en Discord
discord_client_id = ""  # override para forks; vacío = el ID embebido

[lastfm]
enabled = false         # scrobbling (alta: smusic --lastfm-auth)
api_key = ""
secret = ""
```
