use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Clear, List, ListItem, Paragraph};

use super::playlists::{block, centered};
use crate::app::{App, Overlay};

/// Paleta de comandos (`:`): input arriba, coincidencias debajo (máx. 8).
pub fn draw(frame: &mut Frame, app: &mut App) {
    if !matches!(app.overlay, Overlay::Command) {
        return;
    }
    let area = centered(frame.area(), 50, 40);
    frame.render_widget(Clear, area);

    let input_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 3,
    };
    let input = Paragraph::new(format!(":{}", app.overlay_input))
        .block(block("Comandos".to_string(), app.theme.accent));
    frame.render_widget(input, input_area);
    let cursor_x = area.x + 2 + app.overlay_input.chars().count() as u16;
    frame.set_cursor_position((
        cursor_x.min(area.x + area.width.saturating_sub(2)),
        area.y + 1,
    ));

    let matches = crate::app::commands::matching(&app.overlay_input);
    let list_area = Rect {
        x: area.x,
        y: area.y + 3,
        height: area.height.saturating_sub(3),
        ..area
    };
    if matches.is_empty() {
        frame.render_widget(
            Paragraph::new("sin coincidencias").style(Style::new().fg(app.theme.muted)),
            list_area,
        );
        return;
    }
    let items: Vec<ListItem> = matches
        .iter()
        .map(|label| ListItem::new(*label))
        .collect();
    // El primer match es el que ejecutará Enter: se marca como selección.
    let mut first = ratatui::widgets::ListState::default();
    first.select(Some(0));
    let list = List::new(items)
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, list_area, &mut first);

    if area.height >= 3 {
        let hint_area = Rect {
            x: area.x + 1,
            y: area.y + area.height - 1,
            width: area.width.saturating_sub(2),
            height: 1,
        };
        frame.render_widget(
            Paragraph::new("Enter ejecutar el primero · Esc cerrar")
                .style(Style::new().fg(app.theme.muted)),
            hint_area,
        );
    }
}
