## Resumen

<!-- Qué cambia y por qué. Una o dos frases; el detalle va en la descripción. -->

## Issue relacionada

<!-- Cierra #123 / Relacionada con #456. Si no hay issue, explica por qué el cambio es trivial. -->

## Tipo de cambio

- [ ] Bug fix
- [ ] Funcionalidad nueva (issue de propuesta discutida primero)
- [ ] Documentación
- [ ] Refactor / mantenimiento (sin cambio de comportamiento)

## Verificación

Los tres comandos del CI, en verde en local:

- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] `cargo build --release`

Además:

- [ ] Probado a mano en la TUI si el cambio toca UI, teclas o reproducción (indica SO y terminal).
- [ ] Tests nuevos o ajustados para el comportamiento observable que cambia (los de red, con `#[ignore]`).
- [ ] Sin `unsafe` nuevo.
- [ ] Documentación actualizada en el mismo PR si el cambio toca lo que describen README, `docs/`, `SPEC.md` o `AGENTS.md`.

## Notas para quien revisa

<!-- Decisiones de diseño, alternativas descartadas, riesgos, capturas de la TUI si aplica. -->
