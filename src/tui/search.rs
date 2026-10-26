use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph};

use super::fmt_duration;
use crate::app::{App, Focus};

pub fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Searching;
    let provider = app.active_provider_kind();
    let title = if focused {
        format!("Buscar en {provider} (Enter: buscar · Esc/Tab: resultados)")
    } else {
        format!("Buscar en {provider} (/ para enfocar)")
    };
    let border_style = if focused {
        Style::new().fg(app.theme.accent)
    } else {
        Style::default()
    };
    let input = Paragraph::new(Line::from({
        let mut spans = vec![Span::raw(app.input.as_str())];
        // Ghost text: autocompletado desde el historial (→ acepta).
        if focused && let Some(suggestion) = app.history_suggestion() {
            spans.push(Span::styled(
                &suggestion[app.input.len()..],
                Style::new().fg(app.theme.muted),
            ));
        }
        spans
    }))
    .block(Block::bordered().title(title).border_style(border_style));
    frame.render_widget(input, area);
    if focused {
        let cursor_x = area.x + 1 + app.input.chars().count() as u16;
        frame.set_cursor_position((cursor_x.min(area.x + area.width.saturating_sub(2)), area.y + 1));
    }
}

pub fn draw_results(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Browsing;
    let border_style = if focused {
        Style::new().fg(app.theme.accent)
    } else {
        Style::default()
    };
    let title = if app.searching {
        "Resultados (buscando…)".to_string()
    } else {
        format!("Resultados ({})", app.results.len())
    };
    let block = Block::bordered().title(title).border_style(border_style);
    if app.results.is_empty() {
        let hint = if app.searching {
            "Buscando…"
        } else {
            "Escribe una búsqueda y pulsa Enter"
        };
        frame.render_widget(Paragraph::new(hint).block(block), area);
        return;
    }
    let items: Vec<ListItem> = app
        .results
        .iter()
        .map(|track| {
            let playing = app
                .queue
                .current_track()
                .is_some_and(|current| current.provider == track.provider && current.id == track.id);
            let favorite = app.favorites.contains(track);
            let marker = match (playing, favorite) {
                (true, _) => "♪ ",
                (false, true) => "★ ",
                (false, false) => "",
            };
            ListItem::new(format!(
                "{}{} — {} ({})",
                marker,
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
    frame.render_stateful_widget(list, area, &mut app.list_state);
}
