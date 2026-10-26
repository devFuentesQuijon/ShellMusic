# AGENTS.md — ShellMusic

Guía para agentes que trabajan en este repositorio.

## Material de referencia

Consulta estas fuentes ante dudas de diseño o estilo; si una regla del proyecto
las contradice, gana la del proyecto:

- [Pragmatic Rust Guidelines (Microsoft)](https://microsoft.github.io/rust-guidelines/agents/all.txt) — diseño de API, errores, panics, unsafe, FFI, pruebas; pensado para agentes.
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/checklist.html) — convenciones de nombres, conversiones (`as_`/`to_`/`into_`), traits comunes, errores, documentación.
- [Rust API Guidelines — documentación](https://rust-lang.github.io/api-guidelines/) — cómo documentar crates, ejemplos, errores y panics.
- [The Rust Book](https://doc.rust-lang.org/book/) — ownership, borrowing, lifetimes, errores, traits, módulos.
- [Clippy](https://doc.rust-lang.org/clippy/) · [rustfmt](https://github.com/rust-lang/rustfmt) · [cargo](https://doc.rust-lang.org/cargo/) — lint, formato y verificación.
- [rust10x](https://github.com/rust10x/rust10x) — patrones de Rust de producción.

## Reglas aplicadas a este proyecto

ShellMusic es una **aplicación** (binario `smusic`), no una librería pública:

- **Errores**: `anyhow` en toda la app (M-APP-ERROR). Ningún fallo de red,
  parseo o dependencia produce panic; se muestra en la UI o en stderr con
  mensaje accionable (RNF-4 de SPEC.md). Los panics se reservan para bugs de
  programación (M-PANIC-ON-BUG) y llevan mensaje con contexto y valores
  (M-PANIC-MESSAGE). Nada de `catch_unwind` (M-PANIC-IS-STOP).
- **APIs internas idiomáticas**: seguir la checklist de las Rust API
  Guidelines (nombres, conversiones, traits comunes). Cuanto más estándar sea
  el código, más fácil de mantener por humanos y agentes (M-DESIGN-FOR-AI).
- **Documentación**: los módulos llevan `//!` de propósito; los items públicos
  una primera frase de una línea (M-FIRST-DOC-SENTENCE). Sin meta-narrativa de
  diseño en los docs (M-NO-META-DESIGN-DOCUMENTATION): se documenta el estado
  final, no el viaje. Comentarios solo donde el porqué no es obvio.
- **Tests**: el comportamiento observable tiene cobertura (`cargo test`);
  los tests no repiten la implementación (M-TAUTOLOGICAL-TESTS). Los tests que
  requieren red van con `#[ignore]` y corren en el workflow nightly.
- **Extensibilidad**: la TUI y `app` solo conocen los traits `MusicProvider` y
  `Player`; añadir un proveedor o reproductor no toca la UI (regla de SPEC.md §6).
- **Verificación antes de terminar**: `cargo clippy --all-targets -- -D warnings`,
  `cargo test` y `cargo build --release` en verde.
- **unsafe**: prohibido salvo justificación documentada (hoy no hay ninguno).
