# Portabilidad y estadísticas

## Exportar / importar

**Exportar** (`X` desde Resultados/Cola/Favoritos): menú para volcar playlists + favoritos, solo playlists o solo favoritos a un JSON (la ruta admite `~`). En la vista de contenido de una playlist (`P` → `Enter`), `X` la exporta a **M3U extendido** (`#EXTM3U`, `#EXTINF` con duración y «artista - título» y la URL en cada línea).

**Importar** (`I`): por extensión — `.json` fusiona sin duplicados (favoritos por pista, playlists por nombre con merge de pistas), `.m3u`/`.m3u8` crea una playlist con el nombre del archivo; las URLs de YouTube (`watch?v=` / `youtu.be/`) recuperan su id y `#EXTINF` aporta título/artista/duración.

## Estadísticas de escucha

**Estadísticas** (`t`): cada pista que arranca cuenta una reproducción en `stats.json` (mismo directorio de datos que favoritos/playlists, escritura atómica, y guardado periódico de los conteos pendientes). El overlay «Tus más escuchadas» muestra el ranking de pistas (`Enter` reproduce, `a` encola) y, con `Tab`, el de artistas.

Los artistas se agrupan por forma canónica: las variantes que YouTube añade a los canales oficiales («Daft Punk - Topic», «AdeleVEVO», «Artist feat. X») cuentan como el mismo artista, mostrando la forma más frecuente. Es normalización offline, sin red: se evaluó MusicBrainz para esto y quedó descartado por su límite de 1 petición/segundo, inviable en el path de uso.
