# Descubrimiento: autocompletado, letras y «sonando ahora»

- **Autocompletado**: al escribir en la caja de búsqueda, el historial local sugiere el resto de la búsqueda más reciente con ese prefijo (texto atenuado); `→` lo acepta. Todo local, sin red.
- **Letras** (`L`): overlay con la letra de la pista que suena — o de la seleccionada si no hay reproducción — vía [lrclib.net](https://lrclib.net) (sin API key). Se elige la entrada con letra cuya duración más se acerque a la de la pista. Scroll con `↑`/`↓`, `PgUp`/`PgDn` y `g`/`G`.
- **Sonando ahora** (`i`): panel con título, artista, proveedor, posición, volumen, escuchas y la **carátula** de la pista de YouTube, en kitty/sixel si tu terminal lo soporta (Kitty, Konsole, WezTerm, foot…) y con bloques de color como fallback en el resto. En SoundCloud no hay carátula (la API pública no la expone sin credenciales).
