use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Clear, List, ListItem, Paragraph};

use super::fmt_duration;
use crate::app::{App, NameContext, Overlay};

pub fn draw(frame: &mut Frame, app: &mut App) {
    // Los overlays de ayuda, estadísticas y exportación los dibujan sus
    // propios módulos.
    if !matches!(
        app.overlay,
        Overlay::List | Overlay::Content(_) | Overlay::NameInput(_) | Overlay::Pick(_)
    ) {
        return;
    }
    let area = centered(frame.area(), 60, 60);
    // Filas útiles del overlay (menos el borde) para PgUp/PgDn.
    app.overlay_page_size = area.height.saturating_sub(2).max(1) as usize;
    frame.render_widget(Clear, area);
    match app.overlay.clone() {
        Overlay::List => draw_list(frame, app, area),
        Overlay::Content(playlist) => draw_content(frame, app, area, playlist),
        Overlay::NameInput(context) => draw_name_input(frame, app, area, &context),
        Overlay::Pick(track) => draw_pick(frame, app, area, &track),
        _ => {}
    }
}

pub(super) fn centered(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let [vertical] = Layout::vertical([Constraint::Percentage(percent_y)])
        .flex(Flex::Center)
        .areas(area);
    let [horizontal] = Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .areas(vertical);
    horizontal
}

pub(super) fn block(title: String, accent: ratatui::style::Color) -> Block<'static> {
    Block::bordered()
        .title(title)
        .border_style(Style::new().fg(accent))
}

fn draw_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .playlists
        .items
        .iter()
        .map(|playlist| {
            ListItem::new(format!("{} ({})", playlist.name, playlist.tracks.len()))
        })
        .collect();
    let list = List::new(items)
        .block(block("Playlists (P)".to_string(), app.theme.accent))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, area, &mut app.overlay_list_state);
    draw_hints(frame, area, "Enter abrir · n nueva · d eliminar (x2) · Esc cerrar", app.theme.muted);
}

fn draw_content(frame: &mut Frame, app: &mut App, area: Rect, playlist: usize) {
    let Some(playlist_ref) = app.playlists.items.get(playlist) else {
        app.overlay = Overlay::List;
        return;
    };
    let visible = app.visible_content(playlist);
    let total = playlist_ref.tracks.len();
    let filter_note = if visible.len() != total {
        format!("{}/{}", visible.len(), total)
    } else {
        total.to_string()
    };
    let title = format!("Playlist: {} ({filter_note})", playlist_ref.name);
    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(v, real)| {
            let track = &playlist_ref.tracks[*real];
            let playing = app
                .queue
                .current_track()
                .is_some_and(|c| c.provider == track.provider && c.id == track.id);
            let marker = if playing { "♪ " } else { "" };
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
        .block(block(title, app.theme.accent))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, area, &mut app.overlay_content_state);

    // Línea de filtro en el borde inferior (mismo patrón que favoritos).
    if !matches!(app.content_filter_state, crate::app::FilterState::Off) {
        let filter_area = Rect {
            x: area.x + 1,
            y: area.y + area.height.saturating_sub(1),
            width: area.width.saturating_sub(2),
            height: 1,
        };
        let cursor = if matches!(app.content_filter_state, crate::app::FilterState::Editing) {
            "_"
        } else {
            ""
        };
        frame.render_widget(
            Paragraph::new(format!("/{}{}", app.content_filter, cursor))
                .style(Style::new().fg(app.theme.highlight)),
            filter_area,
        );
        draw_hints(
            frame,
            area,
            "Enter/a/A actúan sobre la lista filtrada · Esc volver",
            app.theme.muted,
        );
    } else {
        draw_hints(
            frame,
            area,
            "Enter reproducir · a cola · / filtrar · A toda · d quitar · J/K mover · Esc volver",
            app.theme.muted,
        );
    }
}

fn draw_name_input(frame: &mut Frame, app: &mut App, area: Rect, context: &NameContext) {
    let title = match context {
        NameContext::Plain => "Nueva playlist".to_string(),
        NameContext::WithTrack(track) => format!("Nueva playlist para: {}", track.title),
        NameContext::SaveQueue => format!("Guardar cola ({} pistas) como playlist", app.queue.len()),
    };
    let input = Paragraph::new(app.overlay_input.as_str()).block(block(title, app.theme.accent));
    frame.render_widget(input, area);
    let cursor_x = area.x + 1 + app.overlay_input.chars().count() as u16;
    frame.set_cursor_position((cursor_x.min(area.x + area.width.saturating_sub(2)), area.y + 1));
}

fn draw_pick(frame: &mut Frame, app: &mut App, area: Rect, track: &crate::providers::Track) {
    let mut items: Vec<ListItem> = app
        .playlists
        .items
        .iter()
        .map(|playlist| {
            ListItem::new(format!("{} ({})", playlist.name, playlist.tracks.len()))
        })
        .collect();
    items.push(ListItem::new("+ nueva playlist…"));
    let list = List::new(items)
        .block(block(format!("Añadir a playlist: {}", track.title), app.theme.accent))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, area, &mut app.overlay_list_state);
    draw_hints(frame, area, "Enter elegir · Esc cancelar", app.theme.muted);
}

fn draw_hints(frame: &mut Frame, area: Rect, hints: &str, muted: ratatui::style::Color) {
    if area.height < 3 {
        return;
    }
    let hint_area = Rect {
        x: area.x + 1,
        y: area.y + area.height - 2,
        width: area.width.saturating_sub(2),
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(hints).style(Style::new().fg(muted)),
        hint_area,
    );
}
