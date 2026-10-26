use ratatui::Frame;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::playlists::{block, centered};
use crate::app::{App, Overlay};

const SECTIONS: &[(&str, &[(&str, &str)])] = &[
    (
        "Reproducción",
        &[
            ("Espacio", "play / pausa"),
            ("s", "stop"),
            ("← / →", "seek ∓5 s (Shift: ∓30 s)"),
            ("+ / -", "volumen"),
            ("n / p", "siguiente / anterior de la cola"),
            ("l", "repetición: off → cola → pista"),
            ("S", "barajar la cola"),
        ],
    ),
    (
        "Resultados",
        &[
            ("Enter", "reproducir selección"),
            ("a", "añadir a la cola"),
            ("f", "marcar favorito"),
            ("A", "añadir a playlist"),
        ],
    ),
    (
        "Cola",
        &[
            ("Enter", "reproducir pista"),
            ("d", "eliminar de la cola"),
            ("J / K", "mover abajo / arriba"),
        ],
    ),
    (
        "Favoritos",
        &[
            ("Enter / a", "reproducir / encolar"),
            ("d / f", "quitar de favoritos"),
        ],
    ),
    (
        "Descubrimiento",
        &[
            ("→", "autocompletar búsqueda desde el historial"),
            ("L", "letra de la pista (lrclib.net)"),
            ("i", "panel «sonando ahora» con carátula"),
        ],
    ),
    (
        "General",
        &[
            ("Tab / Shift+Tab", "ciclar paneles"),
            ("PgUp / PgDn", "saltar una página en listas"),
            ("g / G", "inicio / fin de la lista"),
            ("/", "foco a búsqueda"),
            ("m", "cambiar proveedor"),
            ("r / R", "radio on/off · sub-modo"),
            ("P", "gestor de playlists"),
            ("t", "estadísticas de escucha"),
            ("X / I", "exportar / importar datos"),
            ("y", "copiar título, artista y URL de la pista"),
            ("?", "esta ayuda"),
            ("q / Ctrl+C", "salir"),
        ],
    ),
];

pub fn draw(frame: &mut Frame, app: &App) {
    if !matches!(app.overlay, Overlay::Help) {
        return;
    }
    let area = centered(frame.area(), 70, 80);
    frame.render_widget(Clear, area);
    let mut lines = Vec::new();
    for (title, bindings) in SECTIONS {
        lines.push(Line::from(Span::styled(
            *title,
            Style::new().fg(app.theme.accent).add_modifier(Modifier::BOLD),
        )));
        for (key, action) in *bindings {
            lines.push(Line::from(vec![
                Span::styled(format!("  {key:<16}", key = key), Style::new().fg(app.theme.highlight)),
                Span::raw(*action),
            ]));
        }
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        "Esc / ? cerrar",
        Style::new().fg(app.theme.muted),
    )));
    frame.render_widget(
        Paragraph::new(lines).block(block("Ayuda — atajos de teclado".to_string(), app.theme.accent)),
        area,
    );
}
