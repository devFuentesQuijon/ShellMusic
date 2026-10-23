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
    let block = Block::bordered()
        .title(format!("Favoritos ({})", app.favorites.tracks.len()))
        .border_style(border_style);
    if app.favorites.tracks.is_empty() {
        frame.render_widget(
            Paragraph::new("Vacío — marca con 'f' en Resultados o Cola").block(block),
            area,
        );
        return;
    }
    let items: Vec<ListItem> = app
        .favorites
        .tracks
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let playing = app
                .queue
                .current_track()
                .is_some_and(|c| c.provider == track.provider && c.id == track.id);
            let marker = if playing { "♪ " } else { "★ " };
            ListItem::new(format!(
                "{}{}. {} — {} ({})",
                marker,
                i + 1,
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
}
