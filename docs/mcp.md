# ShellMusic para agentes de IA (MCP)

ShellMusic expone un **socket de control local** (RF-63) y un **puente MCP**
(RF-64, binario `smusic-mcp`) para que agentes de IA —Claude Code, Cursor,
etc.— puedan buscar, reproducir y gestionar la cola de la app en ejecución.
Todo es local, opt-in y sin servicios en la nube.

## Activación

```toml
# config.toml
[mcp]
enabled = true
```

Solo se crea el socket con la app **en ejecución** y `enabled = true`. Si no
se puede crear (otra instancia activa, sin permisos), la app arranca igual y
lo avisa en la línea de estado.

## Configurar el agente

Registra `smusic-mcp` como servidor MCP (JSON-RPC 2.0 por stdio). Ejemplo
para Claude Code en `~/.claude.json` (o `claude mcp add`):

```json
{
  "mcpServers": {
    "shellmusic": {
      "command": "/ruta/al/smusic-mcp"
    }
  }
}
```

El binario `smusic-mcp` se distribuye junto a `smusic` en
[GitHub Releases](https://github.com/devFuentesQuijon/ShellMusic/releases)
(`smusic-mcp-<plataforma>`); si compilas desde fuente, `cargo install --path .`
lo instala también (es el segundo binario del paquete).

## Tools disponibles (13)

| Tool | Descripción |
|---|---|
| `search` | Busca (`query`) y devuelve la lista de resultados con título, artista, duración, URL y proveedor. |
| `play` | Busca (`query`) y reproduce la primera coincidencia al instante. |
| `now_playing` | Estado actual: pista, posición, pausa, volumen, índice/tamaño de cola, radio, repetición. |
| `queue_list` | La cola completa con el índice de la pista actual. |
| `queue_add` | Busca (`query`) y encola la primera coincidencia sin reproducirla. |
| `queue_remove` | Elimina la pista de la cola con el `index` dado (0-based). |
| `queue_play` | Reproduce la pista de la cola con el `index` dado, sin buscar. |
| `queue_clear` | Vacía la cola y detiene la reproducción. |
| `control` | Ejecuta un comando de la paleta (`:`): `pause`, `stop`, `next`, `prev`, `vol up`, `vol down`, `vol 80`, `theme dracula`, `radio on`, `radio off`, `stats`, `playlists`, `help`, `quit`. |
| `radio` | Activa/desactiva (`enabled`) el modo radio. |
| `playlists_list` | Lista las playlists locales con su número de pistas. |
| `playlist_play` | Sustituye la cola por la playlist (`name`) y reproduce su primera pista. |
| `favorites_list` | La lista de favoritos. |

Los resultados llegan como texto JSON compacto (campo `data` del protocolo de
control), listos para que el agente los procese.

## Cómo funciona

```
agente ⇄ stdio (JSON-RPC 2.0) ⇄ smusic-mcp ⇄ socket de control ⇄ smusic
```

- **Socket de control** (`src/control.rs`): un JSON por línea en cada
  dirección (`{"req": …}` → `{"ok": true, "data": …}` o
  `{"ok": false, "error": …}`) sobre un socket Unix en directorio `0700`
  (`$XDG_RUNTIME_DIR/shellmusic/control.sock`, con caídas a `/run/user/{uid}`
  y al temporal privado) o un named pipe en Windows
  (`\\.\pipe\shellmusic-control-{usuario}`).
- **Puente** (`src/bin/smusic-mcp.rs`): implementa MCP a mano con
  `serde_json` (sin SDK). Parámetros inválidos → error JSON-RPC `-32602`;
  fallos de la app o de conexión → tool con `isError: true` y mensaje
  accionable; timeout de 30 s por tool.
- Las tools que implican búsqueda (`search`, `play`, `queue_add`) responden
  cuando llega el resultado; solo hay una petición de control en vuelo (el
  puente serializa las llamadas del agente).

## Seguridad

El socket vive en un directorio con permisos `0700`: solo procesos del
**mismo usuario** pueden hablar con la app (mismo modelo que el IPC de mpv o
de Discord). No hay autenticación adicional ni red: cualquier proceso tuyo
puede controlar la reproducción mientras el socket existe. Desactiva
`[mcp] enabled` si no lo usas: el socket ni se crea.

## Resolución de problemas

| Síntoma | Causa |
|---|---|
| `isError`: «no se pudo conectar al socket de control» | `smusic` no está en ejecución, o `[mcp] enabled = false`. |
| «ya hay otra instancia de ShellMusic con el control activo» | Otra sesión de `smusic` tiene el socket; ciérrala o desactiva `[mcp]` en una de las dos. |
| `isError`: «hay una búsqueda en curso» | La TUI está buscando; reintenta en unos segundos. |
| La tool tarda ~30 s y falla | Red lenta o bloqueo anti-bot de YouTube; actualiza yt-dlp (véase [anti-bot.md](anti-bot.md)). |
