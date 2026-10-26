# Bloqueo anti-bot de YouTube

YouTube bloquea la resolución de streams desde muchas IPs (HTTP 429 "Sign in to confirm you're not a bot" o HTTP 403 al abrir el stream). La búsqueda suele funcionar, pero la reproducción falla. ShellMusic defiende en cadena:

1. **Cliente emulado**: empieza por `player_client` (android por defecto); si YouTube lo bloquea, rota por `ios` → `tv` → `web_embedded`.
2. **Cookies**: ante bloqueo, cada cliente reintenta con las cookies de cada navegador instalado detectado (firefox primero). La combinación cliente × cookies que funciona se recuerda durante la sesión; si caduca, se descarta y se recorre el plan completo de nuevo.
3. **PO Token**: si instalas el plugin de yt-dlp (`pip install bgutil-ytdlp-pot-provider`), el primer intento usa el cliente por defecto con PO Token, que suele dar mejores streams. Se desactiva con `[youtube] pot_provider = "off"`.
4. **yt-dlp al día**: los parches anti-bot llegan con cada release; si tu yt-dlp tiene más de 90 días, la app te lo advierte al arrancar.

Detalle de la configuración en la sección `[youtube]` de `config.toml`:

- `cookies_from_browser`:
  - `"auto"` (por defecto): reproduce primero **sin cookies**; solo si YouTube bloquea el stream, reintenta con las cookies de cada navegador instalado detectado (firefox, chrome, chromium, brave, edge, opera, vivaldi — firefox primero). Si todo falla, la UI te pide iniciar sesión en YouTube desde algún navegador instalado (basta una sesión de visitante, no hace falta cuenta).
  - `"firefox"`, `"chrome"`, ...: usa solo ese navegador, sin auto-detección. Requiere haber iniciado sesión de YouTube en él.
  - `""`: nunca usa cookies.
- `player_client`: cliente preferido; la rotación de fallback es automática.
- `pot_provider`: `"auto"` (por defecto) usa el plugin bgutil si está instalado; `"off"` lo ignora.
- `remote_components`: permite a yt-dlp descargar el solucionador de retos JS de firma desde GitHub; necesita un runtime JS instalado (`bun`, `deno` o `node`).

**Privacidad**: en modo `auto` la app solo lee las cookies del navegador cuando el intento sin cookies falla. Las cookies nunca salen de tu equipo: se pasan únicamente al proceso local de yt-dlp/mpv para autenticar la petición a YouTube.

Si la reproducción falla, el mensaje de error en la barra de estado incluye el motivo real reportado por mpv/yt-dlp y sugiere las tres salidas: iniciar sesión en un navegador, instalar el plugin PO Token o actualizar yt-dlp.
