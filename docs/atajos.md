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
| ? | Resultados / Cola / Favoritos | Ayuda de atajos (overlay; se cierra con Esc o ?) |
| PgUp / PgDn | Listas (resultados, cola, favoritos, playlists) | Saltar una página |
| g / G | Listas (resultados, cola, favoritos, playlists) | Ir al inicio / al final |
| / | Resultados / Cola / Favoritos | Foco a búsqueda |
| / | Favoritos / Contenido de playlist | Filtrar la lista en vivo: escribe y la lista se recorta (título/artista, sin distinguir mayúsculas); Enter opera sobre la lista filtrada, Esc suelta el filtro |
| : | Resultados / Cola / Favoritos | Paleta de comandos: escribe para filtrar los ~15 comandos (`pause`, `vol 80`, `theme dracula`, `sleep 15`…) y Enter ejecuta el primero |
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

## Sleep timer

Desde la paleta de comandos (`:`):

- `: sleep 15` — stop dentro de 15 minutos (también `15m`; `90s` acepta segundos y redondea a minutos).
- `: sleep track` — stop al terminar la pista en curso (no cuenta la repetición «one» ni la radio: se consume antes).
- `: sleep off` — cancela el timer activo.

El tiempo restante se muestra en la barra del reproductor (`⏳mm:ss` o `⏳fin pista`).

## Ratón

Activo por defecto (`[general] mouse = true`; con `false` la selección de texto del terminal sigue funcionando):

- **Click en la barra del reproductor**: seek a la fracción de la duración del punto pulsado.
- **Rueda arriba / abajo**: volumen ±5.
- **Click en un panel**: mueve el foco; **click en una fila**: selecciona esa pista.
- Sobre un overlay abierto el ratón no actúa; mover el ratón no repinta la UI.

## Overlay de playlists (modal; captura todas las teclas salvo Ctrl+C)

| Tecla | Vista | Acción |
|---|---|---|
| ↑ / ↓ (o k / j) | Todas | Mover selección |
| n | Lista | Nueva playlist (pide nombre) |
| d | Lista | Eliminar playlist (pulsa `d` dos veces para confirmar) |
| Enter | Lista | Abrir contenido |
| Enter | Contenido | Reproducir la pista (la añade a la cola) |
| a | Contenido | Añadir la pista a la cola sin interrumpir |
| A | Contenido | Reproducir la playlist entera (encola todo y suena la primera) |
| d | Contenido | Quitar la pista de la playlist (se deshace con `u`) |
| u | Contenido | Deshacer el último borrado |
| J / K | Contenido | Reordenar la pista |
| Esc | Lista / Contenido / Selector | Cerrar / volver / cancelar |

## Overlay de letras

Con letra sincronizada (la línea actual va marcada con ♪ y el scroll la sigue):

- `f` activa/desactiva el seguimiento de la reproducción.
- Cualquier scroll manual (`↑/↓`, `PgUp/PgDn`, `g/G`) desactiva el seguimiento.
- `Esc` / `L` cierra.
