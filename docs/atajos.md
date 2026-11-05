# Atajos de teclado

Mientras se edita la búsqueda, las teclas se capturan como texto; los controles de reproducción aplican con el foco en Resultados, Cola o Favoritos.

## Paneles principales

| Tecla | Contexto | Acción |
|---|---|---|
| Texto + Enter | Búsqueda | Ejecutar búsqueda |
| ↑ / ↓ | Búsqueda | Navegar el historial de búsquedas recientes (al bajar de la más reciente se restaura lo que estabas escribiendo) |
| → | Búsqueda | Aceptar el autocompletado del historial (ghost text) |
| Esc / Tab | Búsqueda | Pasar a resultados |
| Tab / Shift+Tab | Resultados / Cola / Favoritos | Ciclar foco: Búsqueda → Resultados → Cola → Favoritos |
| ↑ / ↓ (o k / j) | Resultados / Cola / Favoritos | Mover selección |
| Enter | Resultados | Reproducir selección (la añade a la cola si no está) |
| a | Resultados | Añadir a la cola sin interrumpir la reproducción |
| e | Resultados / Favoritos | Encolar a continuación: insertar justo detrás de la pista que suena (si ya estaba en la cola, la mueve) |
| w | Cola | Guardar la cola como playlist (pide nombre; sugiere «cola YYYY-MM-DD») |
| f | Resultados / Cola | Marcar / quitar favorito (o de la pista que suena si no hay selección) |
| Enter | Cola | Reproducir la pista seleccionada |
| d | Cola | Eliminar de la cola (se deshace con `u`) |
| c | Cola | Ir a la pista que suena (la lista se desplaza hasta ella) |
| J / K | Cola | Mover la pista abajo / arriba |
| e | Cola | Mover la pista seleccionada a «a continuación» (detrás de la que suena) |
| Enter | Favoritos | Reproducir (y añadir a la cola) |
| a | Favoritos | Añadir a la cola sin interrumpir |
| d / f | Favoritos | Quitar de favoritos (se deshace con `u`) |
| Espacio | Resultados / Cola / Favoritos | Play / pausa |
| y | Resultados / Cola / Favoritos | Copiar título, artista y URL de la pista (la que suena o la seleccionada) al portapapeles |
| s | Resultados / Cola / Favoritos | Stop |
| ← / → | Resultados / Cola / Favoritos | Seek ∓5 s dentro de la pista (con Shift: ∓30 s) |
| 0–9 | Resultados / Cola / Favoritos | Seek al 0–90 % de la pista (como mpv: `5` a la mitad, `0` al inicio) |
| + / - | Resultados / Cola / Favoritos | Volumen arriba / abajo |
| n / p | Resultados / Cola / Favoritos | Siguiente / anterior pista de la cola (sin bucle) |
| l | Resultados / Cola / Favoritos | Ciclar repetición: off → toda la cola → pista actual |
| S | Resultados / Cola / Favoritos | Barajar la cola (la pista que suena mantiene su lugar) |
| u | Resultados / Cola / Favoritos / Contenido de playlist | Deshacer el último borrado (cola, favoritos o playlist): reinserta la pista en su posición |
| o | Resultados / Cola / Favoritos | Buscar pistas del artista de la pista seleccionada (o de la que suena) |
| B | Resultados / Cola / Favoritos | Radio basada en la pista seleccionada (o la que suena): la próxima expansión de radio se siembra desde ella; enciende la radio si estaba apagada |
| ? | Resultados / Cola / Favoritos | Ayuda de atajos (overlay; se cierra con Esc o ?) |
| PgUp / PgDn | Listas (resultados, cola, favoritos, playlists) | Saltar una página |
| g / G | Listas (resultados, cola, favoritos, playlists) | Ir al inicio / al final |
| / | Resultados / Cola / Favoritos | Foco a búsqueda; en Cola o Favoritos filtra la lista en vivo (Esc suelta el filtro) |
| / | Favoritos / Contenido de playlist | Filtrar la lista en vivo: escribe y la lista se recorta (título/artista, sin distinguir mayúsculas); Enter opera sobre la lista filtrada, Esc suelta el filtro |
| : | Resultados / Cola / Favoritos | Paleta de comandos: escribe para filtrar los ~21 comandos (`pause`, `vol 80`, `theme dracula`, `sleep 15`, `sort title`, `ytm home`…) y Enter ejecuta el primero |
| q | Resultados / Cola / Favoritos | Salir |
| P | Resultados / Cola / Favoritos | Abrir gestor de playlists (overlay) |
| A | Resultados / Cola / Favoritos | Añadir la pista seleccionada (o la que suena) a una playlist |
| m | Resultados / Cola / Favoritos | Cambiar de proveedor de búsqueda (YouTube / SoundCloud) |
| t | Resultados / Cola / Favoritos | Estadísticas de escucha (overlay; Tab cicla pistas → artistas → historial) |
| X | Resultados / Cola / Favoritos | Exportar playlists/favoritos a JSON (en una playlist: a M3U) |
| I | Resultados / Cola / Favoritos | Importar desde `.json` o `.m3u` |
| L | Resultados / Cola / Favoritos | Letra de la pista que suena (o la seleccionada); si lrclib trae `syncedLyrics`, la línea actual se resalta y sigue a la reproducción |
| i | Resultados / Cola / Favoritos | Panel «sonando ahora» con metadata y carátula |
| r | Resultados / Cola / Favoritos | Activar / desactivar el modo radio |
| R | Resultados / Cola / Favoritos | Ciclar el sub-modo de radio: artista → similares → random → random género → rarezas |
| Ctrl+C | Global | Salir |

Los mensajes informativos de la línea de estado (favorito añadido, copiado, volumen…) caducan a los ~6 s y vuelve la ayuda contextual; los errores persisten hasta la siguiente acción.

## Barra del reproductor

Además del progreso y los metadatos habituales (cola, volumen, repetición, radio, sleep timer) muestra el contexto de la cola:

- `→ título` — la pista que sonará a continuación.
- `+mm:ss` — duración restante de la cola (suma solo las pistas con duración conocida).

Si una pista falla al cargar (stream caído), la app avisa y pasa automáticamente a la siguiente; tras 3 saltos seguidos sin éxito se detiene con el error a mano, por si el fallo es del servicio y no de la pista.

## Sleep timer

Desde la paleta de comandos (`:`):

- `: sleep 15` — stop dentro de 15 minutos (también `15m`; `90s` acepta segundos y redondea a minutos).
- `: sleep track` — stop al terminar la pista en curso (no cuenta la repetición «one» ni la radio: se consume antes).
- `: sleep off` — cancela el timer activo.

El tiempo restante se muestra en la barra del reproductor (`⏳mm:ss` o `⏳fin pista`).

## Ordenar listas

- `: sort title` / `: sort artist` — ordenan el panel con el foco (Cola o Favoritos) por título o por artista, insensible a mayúsculas; la pista que suena conserva su lugar en la cola.
- Dentro del contenido de una playlist: `s` ordena por título y `S` por artista (persistente).

## Ratón

Activo por defecto (`[general] mouse = true`; con `false` la selección de texto del terminal sigue funcionando):

- **Click en la barra del reproductor**: seek a la fracción de la duración del punto pulsado.
- **Rueda arriba / abajo**: volumen ±5.
- **Click en un panel**: mueve el foco; **click en una fila**: selecciona esa pista.
- Sobre un overlay abierto el ratón no actúa; mover el ratón no repinta la UI.

## Overlay de playlists (modal; captura todas las teclas salvo Ctrl+C)

Las entradas `✦` del listado son **playlists inteligentes**: listas virtuales derivadas de tus estadísticas y favoritos (sin persistencia propia). Se ocultan mientras están vacías:

- `✦ Más escuchadas` — top por reproducciones (tope 100).
- `✦ Sin estrenar` — favoritos sin ninguna reproducción registrada.
- `✦ Redescubrir` — última escucha hace más de 30 días, las más olvidadas primero (tope 100).

Su contenido es de **solo lectura**: reproducir, encolar, filtrar, ordenar la vista y exportar a M3U funcionan; quitar, mover, renombrar y eliminar quedan desactivados con un aviso.

| Tecla | Vista | Acción |
|---|---|---|
| ↑ / ↓ (o k / j) | Todas | Mover selección |
| n | Lista | Nueva playlist (pide nombre) |
| d | Lista | Eliminar playlist guardada (pulsa `d` dos veces; las inteligentes no se eliminan) |
| Enter | Lista | Abrir contenido |
| Enter | Contenido | Reproducir la pista (la añade a la cola) |
| a | Contenido | Añadir la pista a la cola sin interrumpir |
| A | Contenido | Reproducir la lista entera (encola todo y suena la primera) |
| d | Contenido | Quitar la pista de la playlist guardada (se deshace con `u`; en inteligentes: aviso de solo lectura) |
| u | Contenido | Deshacer el último borrado |
| s / S | Contenido | Ordenar por título / por artista (persistente en guardadas; de vista en inteligentes) |
| J / K | Contenido | Reordenar la pista (solo guardadas) |
| Esc | Lista / Contenido / Selector | Cerrar / volver / cancelar |

## Comandos headless (sin TTY)

Con `smusic` en ejecución y `[mcp] enabled` en `config.toml`, el binario controla la instancia desde scripts, atajos del escritorio o lanzadores — sin abrir la TUI:

```
smusic play <búsqueda>   reproduce la primera coincidencia
smusic add <búsqueda>    la añade a la cola
smusic status            «♪ título — artista (m:ss/m:ss)» (⏸ en pausa; «nada sonando» si la cola está vacía)
smusic pause | next | prev | stop
smusic … --json          respuesta cruda del socket, para scripts
```

Salida legible en stdout y códigos de salida: 0 ok, 1 error (con mensaje accionable en stderr; p. ej. arrancar la app o activar `[mcp]`), 2 uso incorrecto. Los subcomandos de búsqueda esperan hasta 30 s; el control directo, 5 s. No se combinan con `-t`/`-p`/`--lastfm-auth` (esos arrancan la TUI).

## SponsorBlock

Opt-in (`[sponsorblock] enabled = true`): al arrancar una pista de YouTube se piden los segmentos marcados por la comunidad (sponsor, intro, outro…) y, al llegar la reproducción a uno, se salta a su final con un aviso en la línea de estado. Oro en mixes largos.

- **Privacidad**: habilitarlo envía el ID de cada vídeo que reproduces a sponsor.ajay.app (terceros), como indica el propio `config.toml`.
- Categorías en `[sponsorblock] categories` (añade `music_offtopic` para cortar parlamento de sets de DJ).
- Solo salta si quedan ≥1,5 s de tramo; no molesta con seeks de nada.

## YouTube Music con tu cuenta

Opt-in (`[ytm]`), vía `ytmusicapi` (Python). Alta:

1. `pip install ytmusicapi`
2. `smusic --ytm-auth` — pega las cabeceras copiadas de la petición «browse» de music.youtube.com (F12 → Red → clic derecho → copiar cabeceras) y Enter dos veces.
3. `[ytm] enabled = true` en `config.toml`.

Comandos de paleta (`:`):

- `: ytm likes` — importa tus likes de YouTube Music como favoritos (deduplicado; los existentes se respetan).
- `: ytm import <nombre>` — importa la playlist de tu biblioteca cuyo título contiene `<nombre>` como playlist local (rechaza si ya existe una local homónima).
- `: ytm home` — tus recomendaciones (feed Home) en Resultados.
- `: ytm radio` — radio personalizada de la pista que suena (la watch playlist de tu cuenta) en Resultados; excluye la propia semilla.

Con `[ytm] like_writeback = true` (por defecto), la tecla `f` sobre una pista de YouTube también da/quita like en tu cuenta. Los errores de la integración son accionables: cookies caducadas → rehaz `smusic --ytm-auth`; falta la librería → `pip install ytmusicapi`.

## Overlay de letras

Con letra sincronizada (la línea actual va marcada con ♪ y el scroll la sigue):

- `f` activa/desactiva el seguimiento de la reproducción.
- Cualquier scroll manual (`↑/↓`, `PgUp/PgDn`, `g/G`) desactiva el seguimiento.
- `Esc` / `L` cierra.
