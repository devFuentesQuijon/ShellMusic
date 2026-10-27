# Temas: colección Halloween 🎃

Siete temas listos para probar inspirados en los monstruos clásicos. Para usar uno, edita `config.toml` (ruta en [Configuración](configuracion.md)), sustituye la sección `[theme]` por el bloque del tema y reinicia `smusic`.

Los colores aceptan nombres, hex (`#rrggbb`) e índices de paleta; estos temas usan hex para fidelidad. Pensados para fondos de terminal oscuros.

## Dracula

Púrpura de capa, rosa de neón y rojo sangre. El más legible de la colección.

```toml
[theme]
accent = "#bd93f9"     # púrpura de capa
highlight = "#ff79c6"  # rosa neón
error = "#ff5555"      # rojo sangre
muted = "#6272a4"      # azul grisáceo
progress = "#8be9fd"   # ojos brillantes en la oscuridad
```

## Frankenstein

Verde de laboratorio con chispas eléctricas.

```toml
[theme]
accent = "#00e676"     # piel del monstruo
highlight = "#76ff03"  # electricidad
error = "#ff5252"
muted = "#78909c"
progress = "#69f0ae"
```

## La novia de Frankenstein

Blanco y negro de la película con la línea azul eléctrico de su melena.

```toml
[theme]
accent = "#e0e0e0"     # vendas y película muda
highlight = "#80d8ff"  # rayo azul en la melena
error = "#ff5252"
muted = "#9e9e9e"
progress = "#f5f5f5"
```

## El hombre lobo

Ámbar de luna llena y gris de pelaje.

```toml
[theme]
accent = "#ffd54f"     # luna llena
highlight = "#ff8f00"  # ojos en la noche
error = "#e53935"
muted = "#757575"      # pelaje gris
progress = "#ffca28"
```

## La momia

Vendaje y oro del sarcófago.

```toml
[theme]
accent = "#d7ccc8"     # vendas
highlight = "#ffd54f"  # oro
error = "#e53935"
muted = "#8d6e63"
progress = "#ffca28"
```

## El hombre invisible

La interfaz casi no se ve… como él. Solo el error se atreve a mostrarse.

```toml
[theme]
accent = "#424242"
highlight = "#616161"
error = "#b71c1c"      # lo único visible
muted = "#212121"
progress = "#9e9e9e"
```

## La laguna negra

Verdes de pantano y brillo bajo el agua.

```toml
[theme]
accent = "#00897b"     # agua de la laguna
highlight = "#4db6ac"  # escamas
error = "#ff5252"
muted = "#37474f"      # fondo del lago
progress = "#64ffda"   # brillo bajo el agua
```

## Thriller

Es un reproductor de música: no podía faltar. Rojo chaqueta de Michael, dorado del videoclip, naranja de la caldera.

```toml
[theme]
accent = "#d00000"     # chaqueta
highlight = "#ffb700"  # dorado del videoclip
error = "#ff1744"
muted = "#455a64"      # noche de neón
progress = "#ff6d00"
```

## Probar sin tocar la configuración

Todos los temas de esta colección están empaquetados en el binario y se pueden usar directamente desde la línea de comandos, sin editar `config.toml`:

```sh
smusic --theme dracula
smusic -t thriller --play "thriller michael jackson"   # -t es la abreviatura de --theme
```

`--theme` (o `-t`) acepta cualquiera de estos nombres (dracula, frankenstein, bride, wolfman, mummy, invisible, lagoon, thriller) y tiene prioridad sobre el tema de `config.toml` solo en esa ejecución. `--play` (o `-p`) busca al arrancar y suena la primera coincidencia. Con `--help` ves ambas opciones.

## Crear el tuyo

Los cinco roles:

- `accent`: bordes del panel enfocado y títulos de sección.
- `highlight`: mensajes informativos y teclas en la ayuda.
- `error`: mensajes de error.
- `muted`: línea de hints y texto secundario.
- `progress`: barra de progreso.

Consejo: mantén `muted` apagado respecto a `accent` para que los hints no compitan con el foco, y `progress` bien saturado — es la barra que más tiempo miras. Un valor inválido cae al color por defecto con aviso, así que experimentar es seguro.
