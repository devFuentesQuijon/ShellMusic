use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, List, ListItem, Paragraph};

use super::fmt_duration;
use crate::app::{App, Focus};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Queue;
    let border_style = if focused {
        Style::new().fg(app.theme.accent)
    } else {
        Style::default()
    };
    let title = match app.queue.current {
        Some(current) => format!("Cola ({}/{})", current + 1, app.queue.len()),
        None => format!("Cola ({})", app.queue.len()),
    };
    let block = Block::bordered().title(title).border_style(border_style);
    if app.queue.is_empty() {
        frame.render_widget(
            Paragraph::new("Vacía — añade con 'a'").block(block),
            area,
        );
        return;
    }
    let items: Vec<ListItem> = app
        .queue
        .tracks
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let playing = app.queue.current == Some(i);
            let favorite = app.favorites.contains(track);
            let marker = match (playing, favorite) {
                (true, _) => "♪ ",
                (false, true) => "★ ",
                (false, false) => "",
            };
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
    frame.render_stateful_widget(list, area, &mut app.queue_state);
}
