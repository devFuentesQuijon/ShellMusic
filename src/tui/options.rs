//! Dibujo del menú de opciones (RF-70): dos columnas (categorías y filas)
//! estilo btop, con el valor de cada opción a la derecha. Las teclas las
//! maneja `app::options::handle_key`.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, Paragraph};

use super::playlists::{block, centered};
use crate::app::{App, Overlay};
use crate::app::options::{self, CATEGORIES};

pub fn draw(frame: &mut Frame, app: &mut App) {
    if !matches!(app.overlay, Overlay::Options) {
        return;
    }
    let area = centered(frame.area(), 70, 70);
    frame.render_widget(Clear, area);
    let border = super::border_type(app.ui_style);
    let outer = block("Opciones (F2)".to_string(), app.theme.accent, border);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    let [tabs_area, rows_area] =
        Layout::horizontal([Constraint::Length(16), Constraint::Min(20)]).areas(inner);

    // Columna de categorías.
    let tabs: Vec<ListItem> = CATEGORIES
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let style = if i == app.options_tab {
                Style::new()
                    .fg(app.theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(app.theme.muted)
            };
            ListItem::new(format!(" {name}")).style(style)
        })
        .collect();
    frame.render_widget(List::new(tabs), tabs_area);

    // Divisor vertical.
    let divider = Paragraph::new(
        (0..inner.height)
            .map(|_| Line::from("│"))
            .collect::<Vec<_>>(),
    )
    .style(Style::new().fg(app.theme.muted));
    frame.render_widget(
        divider,
        Rect {
            x: rows_area.x.saturating_sub(1),
            y: inner.y,
            width: 1,
            height: inner.height,
        },
    );

    // Filas de la categoría activa: etiqueta a la izquierda, valor a la derecha.
    let rows = options::rows(app, app.options_tab);
    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| {
            ListItem::new(Line::from(vec![
                Span::raw(format!(" {:<28}", row.label)),
                Span::styled(
                    format!("{} ", if row.value == "sí" || row.value == "no" {
                        format!("[{}]", row.value)
                    } else {
                        format!("‹ {} ›", row.value)
                    }),
                    Style::new().fg(app.theme.highlight),
                ),
            ]))
        })
        .collect();
    let list = List::new(items)
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, rows_area, &mut app.options_sel);
}
