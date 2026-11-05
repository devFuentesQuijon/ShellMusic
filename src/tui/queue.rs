use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, List, ListItem, Paragraph};

use super::fmt_duration;
use crate::app::{App, FilterState, Focus};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Queue;
    let border_style = if focused {
        Style::new().fg(app.theme.accent)
    } else {
        Style::default()
    };
    let visible = app.visible_queue();
    let total = app.queue.len();
    // Título: total, o visibles/total cuando el filtro recorta (como favoritos).
    let filter_note = if visible.len() != total {
        format!("{}/{}", visible.len(), total)
    } else {
        total.to_string()
    };
    let title = match app.queue.current {
        Some(current) => format!("Cola ({}/{})", current + 1, filter_note),
        None => format!("Cola ({filter_note})"),
    };
    let block = Block::bordered().title(title).border_style(border_style);
    if app.queue.is_empty() {
        frame.render_widget(Paragraph::new("Vacía — añade con 'a'").block(block), area);
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
            let track = &app.queue.tracks[*real];
            let playing = app.queue.current == Some(*real);
            let favorite = app.favorites.contains(track);
            let marker = match (playing, favorite) {
                (true, _) => "♪ ",
                (false, true) => "★ ",
                (false, false) => "",
            };
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
    frame.render_stateful_widget(list, area, &mut app.queue_state);

    // Línea de filtro en el borde inferior: visible solo con filtro activo.
    if !matches!(app.queue_filter_state, FilterState::Off) {
        let filter_area = Rect {
            x: area.x + 1,
            y: area.y + area.height.saturating_sub(1),
            width: area.width.saturating_sub(2),
            height: 1,
        };
        let cursor = if matches!(app.queue_filter_state, FilterState::Editing) {
            "_"
        } else {
            ""
        };
        frame.render_widget(
            Paragraph::new(format!("/{}{}", app.queue_filter, cursor))
                .style(Style::new().fg(app.theme.highlight)),
            filter_area,
        );
    }
}
