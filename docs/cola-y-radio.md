# Cola, repetición y modo radio

## Cola de reproducción

El panel derecho muestra la cola: posición, título, artista y duración, con `♪` en la pista que suena y `▶` en la seleccionada. La barra del reproductor muestra la posición en la cola (`cola 3/12`).

- Al terminar una pista de forma natural, avanza automáticamente a la siguiente de la cola (autoplay). `s` (stop) no dispara autoplay.
- Si eliminas con `d` la pista que está sonando, se reproduce la que ocupa su lugar; si era la última, se detiene.
- La cola es persistente: se guarda en `session.json` junto a `favorites.json` al cambiar de pista y al salir, y al arrancar se restaura la cola y se reanuda la pista desde la posición en la que estaba. Además, con reproducción activa se guarda periódicamente cada ~30 s: un cierre abrupto (`kill -9`) pierde como máximo 30 segundos.
- Con la radio activa la cola crece sin límite; al superar 500 pistas se podan las ya reproducidas conservando las 20 anteriores a la actual para poder volver con `p`.
- `e` (Resultados / Favoritos / Cola) encola **a continuación**: inserta la pista justo detrás de la que suena, no al final; si ya estaba en la cola la mueve. En el foco Cola mueve la seleccionada a esa posición. Sin nada sonando equivale a `a`.
- `w` (foco Cola) guarda la cola como playlist: pide nombre (sugerido «cola YYYY-MM-DD», editable) y crea la playlist con el contenido en orden.

## Repetición y orden aleatorio

`l` cicla el modo de repetición (la barra del reproductor muestra `rep:all` o `rep:1`):

- **off** (por defecto): al acabar la cola se detiene, o entra el modo radio si está activo.
- **toda la cola**: al terminar la última pista, la cola vuelve a empezar por la primera.
- **pista actual**: repite en bucle la pista que suena.

`S` baraja el orden de la cola; la pista en reproducción conserva su posición. La repetición actúa solo al terminar la pista de forma natural: `n`/`p` manuales siguen sin hacer bucle.

## Modo radio

Con `r` activas el modo radio (estado inicial configurable en `[radio] enabled`; la barra del reproductor muestra `radio:ON·<modo>`). Cuando empieza a sonar la última pista de la cola, ShellMusic busca en segundo plano y encola hasta 5 pistas nuevas según el sub-modo activo (que se cicla con `R`), de modo que al terminar la pista la siguiente ya está en cola y entra sin pausa:

- **artista**: pistas del último artista sonado.
- **similares**: el mix autogenerado de YouTube para la última pista (la radio `RD` real de YouTube; en SoundCloud, fallback a búsqueda por artista + título).
- **random**: un artista al azar de tus favoritos (sin favoritos, cae a random género).
- **random género**: una palabra clave de género/década al azar ("80s rock", "bossa nova", …).
- **rarezas**: pistas menos conocidas del último artista (descarta los primeros hits y baraja el resto).

En todos los modos se excluyen las pistas ya reproducidas o encoladas, y no entran dos versiones de la misma canción (dedupe por título normalizado). Si no hay nada nuevo, la reproducción se detiene al agotarse la cola con un aviso. El modo inicial se configura con `[radio] mode`.
