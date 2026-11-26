# Política de seguridad

## Versiones soportadas

Se atienden vulnerabilidades en la **última release publicada** y en la rama
`main`. Las versiones antiguas no reciben parches: actualiza a la release
más reciente antes de reportar.

## Cómo reportar una vulnerabilidad

**No abras un issue público ni un pull request con detalles del fallo.**

Usa los [avisos de seguridad privados de GitHub](https://github.com/devFuentesQuijon/ShellMusic/security/advisories/new)
(*private vulnerability reporting*). Incluye, en la medida de lo posible:

- Descripción del fallo y su impacto (qué puede hacer un atacante).
- Pasos de reproducción o prueba de concepto.
- Versiones afectadas (release o commit), SO y terminal.
- Si el fallo está en una dependencia, la cadena (`cargo tree -i <crate>`).

Este es un proyecto mantenido por una sola persona: la respuesta es de
mejor esfuerzo. Como referencia, se intenta confirmar la recepción en una
semana y coordinar la divulgación con el informante una vez publicado el
parche. No hay recompensas económicas (*bug bounty*).

## Alcance

Dentro del alcance:

- El binario `smusic` y el puente `smusic-mcp`: parseo de configuración,
  playlists/favoritos/estadísticas en disco, socket de control local
  (`[mcp]`), integración MPRIS/D-Bus, manejo de URLs y respuestas de red.
- Inyección de comandos o argumentos hacia `mpv`/`yt-dlp` a través de
  entradas controladas por el atacante (p. ej. una URL maliciosa).
- Dependencias de crates.io con avisos publicados (RUSTSEC): se corrigen
  actualizando `Cargo.lock`; si el aviso no tiene parche, se documenta la
  mitigación.

Fuera del alcance:

- Vulnerabilidades en `mpv`, `yt-dlp`, los terminales o los servicios de
  terceros (YouTube, SoundCloud, Last.fm, Discord, lrclib.net): repórtalas a
  sus proyectos. Si ShellMusic las expone de forma explotable, eso sí entra.
- Ataques que requieren acceso previo al equipo de la víctima (leer
  `~/.config/shellmusic/`, suplantar el socket local con el mismo UID…):
  la app no es una frontera de seguridad frente a otros procesos del mismo
  usuario.
- El contenido de los themes o configs que el propio usuario instala.

## Prácticas del proyecto

- `cargo audit` sobre `Cargo.lock` ante cada informe de dependencias; los
  avisos con parche se aplican de inmediato.
- CI con clippy en modo estricto (`-D warnings`) y tests en cada PR.
- `unsafe` prohibido en el código del proyecto.
- Errores de red/parseo tratados con `anyhow` (sin panics ante entrada
  hostil).
