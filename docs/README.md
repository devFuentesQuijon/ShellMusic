# Documentación de ShellMusic

Guía completa del reproductor. El [README](../README.md) cubre instalación y una visión general; aquí está todo lo demás.

## Uso

- [Atajos de teclado](atajos.md) — todas las teclas por contexto
- [Cola, repetición y modo radio](cola-y-radio.md)
- [Playlists y favoritos](playlists.md)
- [Descubrimiento: autocompletado, letras y «sonando ahora»](descubrimiento.md)
- [Portabilidad y estadísticas](portabilidad.md)

## Configuración y plataforma

- [Configuración (`config.toml`) y temas](configuracion.md)
- [Temas: colección Halloween](temas.md) — Dracula, Frankenstein, la novia, el hombre lobo, la momia, el hombre invisible, la laguna negra y Thriller
- [Integración con el sistema](integracion-sistema.md) — notificaciones, MPRIS, matriz por SO
- [ShellMusic para agentes de IA (MCP)](mcp.md) — socket de control opt-in y puente `smusic-mcp` para agentes

## Referencia

- **Manual**: `man smusic` — la página de manual está en el repo (`docs/smusic.1`); para instalarla:
  ```sh
  sudo cp docs/smusic.1 /usr/local/share/man/man1/
  sudo mandb   # Debian/Ubuntu; en otras distribuciones basta con copiarla
  ```
- [Bloqueo anti-bot de YouTube](anti-bot.md) — la cadena de defensas y qué hacer cuando falla
- [Aviso legal](legal.md)
- [Desarrollo](desarrollo.md) — tests, CI y verificación multiplataforma
