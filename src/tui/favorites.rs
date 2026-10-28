use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, List, ListItem, Paragraph};

use super::fmt_duration;
use crate::app::{App, Focus};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Favorites;
    let border_style = if focused {
        Style::new().fg(app.theme.accent)
    } else {
        Style::default()
    };
    let visible = app.visible_favorites();
    let total = app.favorites.tracks.len();
    // Título: total, o visibles/total cuando el filtro recorta.
    let filter_note = if visible.len() != total {
        format!("{}/{}", visible.len(), total)
    } else {
        total.to_string()
    };
    let block = Block::bordered()
        .title(format!("Favoritos ({filter_note})"))
        .border_style(border_style);
    if app.favorites.tracks.is_empty() {
        frame.render_widget(
            Paragraph::new("Vacío — marca con 'f' en Resultados o Cola").block(block),
            area,
        );
        return;
    }
    if visible.is_empty() {
        frame.render_widget(
            Paragraph::new("Sin coincidencias — Esc limpia el filtro").block(block),
            area,
        );
        return;
    }
    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(v, real)| {
            let track = &app.favorites.tracks[*real];
            let playing = app
                .queue
                .current_track()
                .is_some_and(|c| c.provider == track.provider && c.id == track.id);
            let marker = if playing { "♪ " } else { "★ " };
            ListItem::new(format!(
                "{}{}. {} — {} ({})",
                marker,
                v + 1,
                track.title,
                track.artist,
                fmt_duration(track.duration)
            ))
        })
        .collect();
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, area, &mut app.favorites_state);

    // Línea de filtro en el borde inferior: visible solo con filtro activo.
    if !matches!(app.favorites_filter_state, crate::app::FilterState::Off) {
        let filter_area = Rect {
            x: area.x + 1,
            y: area.y + area.height.saturating_sub(1),
            width: area.width.saturating_sub(2),
            height: 1,
        };
        let cursor = if matches!(app.favorites_filter_state, crate::app::FilterState::Editing) {
            "_"
        } else {
            ""
        };
        frame.render_widget(
            Paragraph::new(format!("/{}{}", app.favorites_filter, cursor))
                .style(Style::new().fg(app.theme.highlight)),
            filter_area,
        );
    }
}
