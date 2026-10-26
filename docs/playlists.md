# Playlists y favoritos

## Playlists

Playlists locales gestionadas desde el overlay modal (`P`): crear (`n`), eliminar (`d` dos veces), abrir (`Enter`) y, dentro, reordenar (`J`/`K`), quitar pistas (`d`) o reproducir todo (`A`). Con `A` sobre cualquier pista la añades a una playlist existente o a una nueva (sin duplicados por pista). Los nombres de playlist son únicos.

Se guardan en `playlists.json` junto a `favorites.json` (`~/.local/share/shellmusic/playlists.json` en Linux, `~/Library/Application Support/shellmusic/playlists.json` en macOS, `%APPDATA%\shellmusic\playlists.json` en Windows), con escritura atómica y recuperación ante archivo corrupto (`.bak`).

## Favoritos

`f` marca o desmarca la pista seleccionada (o la que suena) como favorita; las favoritas llevan marcador `★` en todos los paneles. El panel Favoritos (cuarta vista del ciclo `Tab`) permite reproducirlas (`Enter`), encolarlas (`a`) o quitarlas (`d`).

Se guardan en `favorites.json` en el directorio de datos nativo (`~/.local/share/shellmusic/favorites.json` en Linux, `~/Library/Application Support/shellmusic/favorites.json` en macOS, `%APPDATA%\shellmusic\favorites.json` en Windows), con escritura atómica. Si el archivo está corrupto se aparta a `favorites.json.bak` y se empieza con lista vacía, sin cerrar la app. Todo es local: no hay cuentas ni sincronización.
