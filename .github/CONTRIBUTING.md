# Contribuir a ShellMusic

¡Gracias por tu interés! Este documento resume cómo proponer cambios y qué
se espera de un pull request. Las reglas de diseño detalladas viven en
[AGENTS.md](../AGENTS.md) y la especificación del producto en
[SPEC.md](../SPEC.md); este archivo es la versión para personas.

## Antes de empezar

- Para **bugs**: abre un [issue](https://github.com/devFuentesQuijon/ShellMusic/issues/new/choose)
  con los pasos de reproducción antes de ponerte a arreglarlo, salvo que sea
  un cambio trivial (erratas, comentarios).
- Para **funcionalidades nuevas**: abre primero un issue de propuesta. La
  especificación (SPEC.md) manda: si la idea contradice una regla de
  producto, hay que discutirla antes de escribir código.
- Para **vulnerabilidades de seguridad**: no abras un issue público; sigue
  la [política de seguridad](SECURITY.md).

## Requisitos del entorno

- Rust stable (la toolchain del CI).
- `mpv` y `yt-dlp` en el `PATH` para probar la app de verdad; los tests que
  los necesitan van marcados con `#[ignore]` y no corren en `cargo test`
  normal (los de red corren en el workflow nightly).

## Flujo de trabajo

1. Haz fork y crea una rama desde `main`.
2. Haz el cambio: un solo tema por PR. Los cambios grandes, mejor en varios
   PRs encadenados que en uno gigante.
3. Verifica en local (ver abajo) y abre el PR contra `main` rellenando la
   plantilla.

## Verificación obligatoria antes de enviar

Los tres comandos deben terminar en verde; el CI los ejecuta en Linux,
macOS y Windows:

```sh
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

## Reglas del proyecto

- **Errores**: `anyhow` en toda la app. Ningún fallo de red, parseo o
  dependencia externa produce panic: se muestra en la UI o en stderr con un
  mensaje accionable. Los panics se reservan para bugs de programación, con
  mensaje que incluya contexto y valores. Nada de `catch_unwind`.
- **Sin `unsafe`**: prohibido salvo justificación documentada (hoy no hay
  ninguno).
- **Extensibilidad**: la TUI y `app` solo conocen los traits
  `MusicProvider` y `Player`. Añadir un proveedor o reproductor no debe
  tocar la UI.
- **Tests**: el comportamiento observable tiene cobertura; los tests no
  repiten la implementación. Tests que requieren red o binarios externos:
  `#[ignore]` con un comentario que diga qué necesitan.
- **Documentación**: los módulos llevan `//!` de propósito; los items
  públicos, una primera frase de una línea. Se documenta el estado final,
  no el viaje. Comentarios solo donde el porqué no es obvio.
- **Idioma**: código, comentarios, docs y mensajes de commit en español.

## Estilo

- APIs internas idiomáticas siguiendo las
  [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/checklist.html).
- Mensajes de commit concisos, en español, que expliquen el cambio
  (p. ej. `fix: remediación CVE — bump de tokio y url`).
- Actualiza la documentación afectada en el mismo PR: README, `docs/`,
  `SPEC.md` o `AGENTS.md` si el cambio toca lo que describen.

## Qué pasa después

El mantenimiento es de mejor esfuerzo: los PRs se revisan sin SLA, pero
todo PR con CI en verde y descripción clara tiene mucho camino ganado. Si
un PR lleva tiempo sin respuesta, un comentario amable lo resucita.
