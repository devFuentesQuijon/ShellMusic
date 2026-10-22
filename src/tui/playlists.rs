use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Clear, List, ListItem, Paragraph};

use super::fmt_duration;
use crate::app::{App, NameContext, Overlay};

pub fn draw(frame: &mut Frame, app: &mut App) {
    // El overlay de ayuda lo dibuja su propio módulo.
    if matches!(app.overlay, Overlay::Closed | Overlay::Help) {
        return;
    }
    let area = centered(frame.area(), 60, 60);
    frame.render_widget(Clear, area);
    match app.overlay.clone() {
        Overlay::Closed | Overlay::Help => {}
        Overlay::List => draw_list(frame, app, area),
        Overlay::Content(playlist) => draw_content(frame, app, area, playlist),
        Overlay::NameInput(context) => draw_name_input(frame, app, area, &context),
        Overlay::Pick(track) => draw_pick(frame, app, area, &track),
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

pub(super) fn block(title: String) -> Block<'static> {
    Block::bordered()
        .title(title)
        .border_style(Style::new().fg(Color::Cyan))
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
        .block(block("Playlists (P)".to_string()))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, area, &mut app.overlay_list_state);
    draw_hints(frame, area, "Enter abrir · n nueva · d eliminar (x2) · Esc cerrar");
}

fn draw_content(frame: &mut Frame, app: &mut App, area: Rect, playlist: usize) {
    let Some(playlist_ref) = app.playlists.items.get(playlist) else {
        app.overlay = Overlay::List;
        return;
    };
    let title = format!("Playlist: {} ({})", playlist_ref.name, playlist_ref.tracks.len());
    let items: Vec<ListItem> = playlist_ref
        .tracks
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let playing = app
                .queue
                .current_track()
                .is_some_and(|c| c.provider == track.provider && c.id == track.id);
            let marker = if playing { "♪ " } else { "" };
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
        .block(block(title))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, area, &mut app.overlay_content_state);
    draw_hints(
        frame,
        area,
        "Enter reproducir · a cola · A toda · d quitar · J/K mover · Esc volver",
    );
}

fn draw_name_input(frame: &mut Frame, app: &mut App, area: Rect, context: &NameContext) {
    let title = match context {
        NameContext::Plain => "Nueva playlist".to_string(),
        NameContext::WithTrack(track) => format!("Nueva playlist para: {}", track.title),
    };
    let input = Paragraph::new(app.overlay_input.as_str()).block(block(title));
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
        .block(block(format!("Añadir a playlist: {}", track.title)))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, area, &mut app.overlay_list_state);
    draw_hints(frame, area, "Enter elegir · Esc cancelar");
}

fn draw_hints(frame: &mut Frame, area: Rect, hints: &str) {
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
        Paragraph::new(hints).style(Style::new().fg(Color::DarkGray)),
        hint_area,
    );
}
