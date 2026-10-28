pub mod command;
pub mod favorites;
pub mod help;
pub mod nowplaying;
pub mod player_bar;
pub mod playlists;
pub mod queue;
pub mod search;
pub mod stats;

use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::widgets::Paragraph;

use crate::app::{App, Focus, Overlay};

const HELP_SEARCH: &str = "Enter buscar · Tab panel · Esc resultados · Ctrl+C salir";
const HELP_RESULTS: &str =
    "Enter reproducir · a cola · f favorito · y copiar · ←/→ seek · l repetir · S barajar · ? ayuda · q salir";
const HELP_QUEUE: &str =
    "Enter reproducir · d eliminar · J/K mover · y copiar · ←/→ seek · l repetir · S barajar · ? ayuda · q salir";
const HELP_FAVORITES: &str =
    "Enter reproducir · a cola · d/f quitar · y copiar · ←/→ seek · l repetir · ? ayuda · q salir";

pub fn draw(frame: &mut Frame, app: &mut App) {
    let [search_area, middle_area, status_area, player_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(3),
    ])
    .areas(frame.area());
    // Tres columnas: en 80 columnas quedan ~36/22/22; los títulos largos se
    // truncan pero todo sigue alcanzable por teclado (RNF-3).
    let [results_area, queue_area, favorites_area] = Layout::horizontal([
        Constraint::Percentage(40),
        Constraint::Percentage(30),
        Constraint::Percentage(30),
    ])
    .areas(middle_area);
    // Áreas para el hit-testing del ratón: click en barra = seek, click en
    // panel = foco, click en fila = selección.
    app.areas = crate::app::PanelAreas {
        results: results_area,
        queue: queue_area,
        favorites: favorites_area,
        player: player_area,
    };
    // Filas útiles de los paneles (menos el borde) para PgUp/PgDn.
    app.list_page_size = middle_area.height.saturating_sub(2).max(1) as usize;
    search::draw_input(frame, app, search_area);
    search::draw_results(frame, app, results_area);
    queue::draw(frame, app, queue_area);
    favorites::draw(frame, app, favorites_area);
    draw_status(frame, app, status_area);
    player_bar::draw(frame, app, player_area);
    playlists::draw(frame, app);
    help::draw(frame, app);
    command::draw(frame, app);
    stats::draw(frame, app);
    stats::draw_export(frame, app);
    stats::draw_path_input(frame, app);
    nowplaying::draw_lyrics(frame, app);
    nowplaying::draw_now_playing(frame, app);
}

fn draw_status(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let (text, style) = match &app.message {
        Some(msg) if msg.is_error => (msg.text.clone(), Style::new().fg(app.theme.error)),
        Some(msg) => (msg.text.clone(), Style::new().fg(app.theme.highlight)),
        None => (
            match app.overlay {
                Overlay::Closed => match app.focus {
                    Focus::Searching => HELP_SEARCH,
                    Focus::Browsing => HELP_RESULTS,
                    Focus::Queue => HELP_QUEUE,
                    Focus::Favorites => HELP_FAVORITES,
                },
                Overlay::Help => "ayuda: Esc cierra",
                Overlay::Stats => "estadísticas: Tab vista · Enter reproducir · Esc cierra",
                Overlay::Lyrics => "letra: ↑/↓ scroll · Esc cierra",
                Overlay::NowPlaying => "sonando ahora: Esc cierra",
                Overlay::Export | Overlay::PathInput(_) => "exportar/importar: Enter confirmar · Esc cancelar",
                Overlay::Command => "comandos: Enter ejecutar · Esc cerrar",
                _ => "playlists: las teclas se muestran dentro del panel",
            }
            .to_string(),
            Style::new().fg(app.theme.muted),
        ),
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
}

pub fn fmt_duration(duration: Option<Duration>) -> String {
    match duration {
        Some(d) => {
            let secs = d.as_secs();
            format!("{}:{:02}", secs / 60, secs % 60)
        }
        None => "--:--".to_string(),
    }
}
