use ratatui::Frame;
use ratatui::style::{Color, Modifier, Style};
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
        "General",
        &[
            ("Tab / Shift+Tab", "ciclar paneles"),
            ("/", "foco a búsqueda"),
            ("m", "cambiar proveedor"),
            ("r / R", "radio on/off · sub-modo"),
            ("P", "gestor de playlists"),
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
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )));
        for (key, action) in *bindings {
            lines.push(Line::from(vec![
                Span::styled(format!("  {key:<16}", key = key), Style::new().fg(Color::Yellow)),
                Span::raw(*action),
            ]));
        }
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        "Esc / ? cerrar",
        Style::new().fg(Color::DarkGray),
    )));
    frame.render_widget(
        Paragraph::new(lines).block(block("Ayuda — atajos de teclado".to_string())),
        area,
    );
}
