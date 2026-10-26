# Integración con el sistema

Al cambiar de pista, ShellMusic muestra una **notificación de escritorio** con título y artista (Linux, macOS y Windows). En **Linux** además se registra como reproductor **MPRIS** (`org.mpris.MediaPlayer2.shellmusic`): las teclas multimedia del teclado, los widgets del escritorio (GNOME/KDE) y **KDE Connect** pueden pausar, parar, cambiar de pista, hacer seek y ajustar el volumen; compruébalo con `playerctl -l` y `playerctl play-pause`.

Matriz de funcionalidades por SO:

| Funcionalidad | Linux | macOS | Windows |
|---|:-:|:-:|:-:|
| Búsqueda y reproducción | ✅ | ✅ | ✅ |
| Cola persistente, favoritos, playlists, estadísticas | ✅ | ✅ | ✅ |
| Notificaciones de escritorio | ✅ | ✅ | ✅ |
| Media controls (teclas multimedia, widgets, KDE Connect) | ✅ (MPRIS) | — | — |
| Carátula | kitty / sixel | iTerm2 | halfblocks |

La reproducción local está verificada en CI en los tres SO (el test IPC de mpv corre en Linux, macOS y Windows). En macOS/Windows no hay media controls porque exigen una ventana o un event loop de GUI, incompatibles con una TUI pura. En una sesión sin D-Bus (SSH, headless) los media controls se desactivan solos con un aviso en la línea de estado; la app sigue funcionando.

Se desactivan por separado en `config.toml`:

```toml
[integrations]
notifications = true    # notificación de escritorio al cambiar de pista
media_controls = true   # MPRIS en Linux (sin efecto en macOS/Windows)
```
