# Configuración y temas

En el primer arranque se crea `config.toml` en la ruta nativa de configuración (`~/.config/shellmusic/config.toml` en Linux, `~/Library/Application Support/shellmusic/config.toml` en macOS, `%APPDATA%\shellmusic\config.toml` en Windows). Las claves ausentes toman su valor por defecto (compatibilidad hacia atrás).

```toml
[general]
results_per_search = 15   # 1-50
providers = ["youtube", "soundcloud"]   # proveedores habilitados; la tecla m cicla entre ellos
default_provider = "youtube"            # proveedor activo al arrancar (debe estar en la lista)

[audio]
initial_volume = 70       # 0-100

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
notifications = true    # notificación de escritorio al cambiar de pista
media_controls = true   # MPRIS en Linux; ver «Integración con el sistema»
```

- Los proveedores se buscan con yt-dlp (`ytsearch` para YouTube, `scsearch` para SoundCloud); el proveedor activo se muestra en el título de la caja de búsqueda y se cambia en caliente con `m`. Nombres desconocidos en `providers` se ignoran; si la lista queda vacía se usa `["youtube"]`, y si `default_provider` no está en la lista se usa el primero.
- Los colores aceptan nombres (`cyan`, `lightred`, `darkgray`…), hex (`#rrggbb`) e índices de paleta (0-255); un valor inválido usa el color por defecto y avisa en la línea de estado.
- El detalle de `[youtube]` (anti-bot) está en [Bloqueo anti-bot de YouTube](anti-bot.md).

## Temas

La pestaña del terminal muestra «ShellMusic» en reposo y «♪ artista — título · ShellMusic» mientras suena una pista; al salir se restablece el título habitual del terminal.

Los colores de la interfaz se configuran en `[theme]` (tabla de arriba). Cambios recientes de tema se aplican al reiniciar la app.

## Caché de búsquedas

Las búsquedas se cachean en memoria durante la sesión (sección `[cache]`): repetir la misma búsqueda en el mismo proveedor no vuelve a invocar yt-dlp hasta que expira el TTL. No se guarda nada en disco.

## Historial de búsquedas

Con la caja de búsqueda enfocada, `↑`/`↓` recorren las búsquedas recientes (como el historial de un shell): `↑` va hacia las más antiguas y, al bajar con `↓` más allá de la más reciente, se restaura lo que estabas escribiendo. Se guardan las últimas 50 en `history.json` junto a `favorites.json`, con la misma escritura atómica.
