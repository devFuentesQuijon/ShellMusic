pub mod favorites;
pub mod player_bar;
pub mod playlists;
pub mod queue;
pub mod search;

use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;

use crate::app::{App, Focus};

const HELP_SEARCH: &str = "Enter buscar · Tab panel · Esc resultados · Ctrl+C salir";
const HELP_RESULTS: &str =
    "Enter reproducir · a cola · f favorito · A playlist · m proveedor · r radio · Tab · / · q salir";
const HELP_QUEUE: &str =
    "Enter reproducir · d eliminar · J/K mover · f ★ · P playlists · m prov · r radio · n/p · q salir";
const HELP_FAVORITES: &str =
    "Enter reproducir · a cola · d/f quitar · A playlist · m prov · r radio · q salir";

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
    search::draw_input(frame, app, search_area);
    search::draw_results(frame, app, results_area);
    queue::draw(frame, app, queue_area);
    favorites::draw(frame, app, favorites_area);
    draw_status(frame, app, status_area);
    player_bar::draw(frame, app, player_area);
    playlists::draw(frame, app);
}

fn draw_status(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let (text, style) = match &app.message {
        Some(msg) if msg.is_error => (msg.text.clone(), Style::new().fg(Color::Red)),
        Some(msg) => (msg.text.clone(), Style::new().fg(Color::Yellow)),
        None => (
            if !matches!(app.overlay, crate::app::Overlay::Closed) {
                "playlists: las teclas se muestran dentro del panel"
            } else {
                match app.focus {
                    Focus::Searching => HELP_SEARCH,
                    Focus::Browsing => HELP_RESULTS,
                    Focus::Queue => HELP_QUEUE,
                    Focus::Favorites => HELP_FAVORITES,
                }
            }
            .to_string(),
            Style::new().fg(Color::DarkGray),
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
