use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Clear, List, ListItem, Paragraph};

use super::playlists::{block, centered};
use crate::app::{App, Overlay};

pub fn draw(frame: &mut Frame, app: &mut App) {
    if !matches!(app.overlay, Overlay::Stats) {
        return;
    }
    let area = centered(frame.area(), 60, 70);
    app.overlay_page_size = area.height.saturating_sub(3).max(1) as usize;
    frame.render_widget(Clear, area);
    let accent = app.theme.accent;
    let muted = app.theme.muted;

    let view = match app.stats_view {
        crate::app::StatsView::Top => "pistas",
        crate::app::StatsView::Artists => "artistas",
        crate::app::StatsView::History => "historial",
    };
    let header = format!(
        "Tus más escuchadas ({view}) — {} reproducciones · {} pistas · {} artistas",
        app.stats.total_plays(),
        app.stats.entries.len(),
        app.stats.top_artists().len()
    );
    let block_widget = block(header, accent);

    if app.stats.entries.is_empty() {
        frame.render_widget(
            Paragraph::new("Aún no hay estadísticas — reproduce algo primero").block(block_widget),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = match app.stats_view {
        crate::app::StatsView::Artists => app
            .stats
            .top_artists()
            .iter()
            .map(|c| ListItem::new(format!("{}× {}", c.plays, c.artist)))
            .collect(),
        crate::app::StatsView::Top => app
            .stats
            .top_tracks()
            .iter()
            .map(|(entry, _)| {
                ListItem::new(format!("{}× {} — {}", entry.plays, entry.title, entry.artist))
            })
            .collect(),
        crate::app::StatsView::History => app
            .stats
            .history()
            .iter()
            .map(|entry| {
                ListItem::new(format!(
                    "{} · {} — {}",
                    crate::stats::format_timestamp(entry.last_played),
                    entry.title,
                    entry.artist
                ))
            })
            .collect(),
    };
    let list = List::new(items)
        .block(block_widget)
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, area, &mut app.stats_state);

    if area.height >= 3 {
        let hint_area = Rect {
            x: area.x + 1,
            y: area.y + area.height - 2,
            width: area.width.saturating_sub(2),
            height: 1,
        };
        frame.render_widget(
            Paragraph::new("Tab vista · Enter reproducir · a cola · Esc cerrar")
                .style(Style::new().fg(muted)),
            hint_area,
        );
    }
}

/// Menú de exportación JSON y el input de ruta compartido.
pub fn draw_export(frame: &mut Frame, app: &mut App) {
    if !matches!(app.overlay, Overlay::Export) {
        return;
    }
    let area = centered(frame.area(), 50, 30);
    frame.render_widget(Clear, area);
    let items = vec![
        ListItem::new("Playlists + favoritos (JSON)"),
        ListItem::new("Solo playlists (JSON)"),
        ListItem::new("Solo favoritos (JSON)"),
    ];
    let list = List::new(items)
        .block(block("Exportar".to_string(), app.theme.accent))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, area, &mut app.overlay_list_state);
    if area.height >= 3 {
        let hint_area = Rect {
            x: area.x + 1,
            y: area.y + area.height - 2,
            width: area.width.saturating_sub(2),
            height: 1,
        };
        frame.render_widget(
            Paragraph::new("Enter elegir · Esc cancelar").style(Style::new().fg(app.theme.muted)),
            hint_area,
        );
    }
}

pub fn draw_path_input(frame: &mut Frame, app: &mut App) {
    let Overlay::PathInput(action) = &app.overlay else {
        return;
    };
    let area = centered(frame.area(), 60, 20);
    frame.render_widget(Clear, area);
    let title = match action {
        crate::app::PathAction::ExportJson(_) => "Exportar JSON a:".to_string(),
        crate::app::PathAction::ExportM3u(index) => {
            let name = app
                .playlists
                .items
                .get(*index)
                .map_or("playlist", |p| p.name.as_str());
            format!("Exportar «{name}» a M3U:")
        }
        crate::app::PathAction::Import => "Importar desde (.json / .m3u):".to_string(),
    };
    let input = Paragraph::new(app.overlay_input.as_str())
        .block(block(title, app.theme.accent));
    frame.render_widget(input, area);
    let cursor_x = area.x + 1 + app.overlay_input.chars().count() as u16;
    frame.set_cursor_position((cursor_x.min(area.x + area.width.saturating_sub(2)), area.y + 1));
}
