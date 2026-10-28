use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use tokio::sync::mpsc;

use crate::player::Player;
use crate::providers::Track;
use crate::queue::Queue;

use super::AppMsg;
use super::playback::{
    add_next_to_queue, add_to_queue, change_volume, open_lyrics, play_adjacent, play_queue_track,
    play_result, radio_prefetch, remove_from_queue, request_cover, seek_by, seek_to,
    shuffle_queue, spawn_player, submit_search, toggle_favorite,
};
use super::state::{App, FilterState, Focus, Message, NameContext, Overlay, PathAction, StatsView};

const VOLUME_STEP: i16 = 5;
const SEEK_STEP: i32 = 5;
const SEEK_STEP_LARGE: i32 = 30;

/// Evento de ratón: click en la barra = seek absoluto por posición; rueda =
/// volumen; click en panel = foco y click en fila = selección. Solo se
/// procesa con el overlay cerrado (sobre un overlay el ratón no actúa).
/// Devuelve true si el estado cambió y hay que repintar.
pub(super) fn handle_mouse(
    app: &mut App,
    mouse: MouseEvent,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
) -> bool {
    if !matches!(app.overlay, Overlay::Closed) {
        return false;
    }
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            change_volume(app, player, tx, VOLUME_STEP);
            true
        }
        MouseEventKind::ScrollDown => {
            change_volume(app, player, tx, -VOLUME_STEP);
            true
        }
        MouseEventKind::Down(MouseButton::Left) => {
            let pos = (mouse.column, mouse.row);
            if app.areas.player.contains(pos.into()) {
                seek_from_bar_click(app, player, tx, mouse.column);
                true
            } else if app.areas.results.contains(pos.into()) {
                clear_favorites_filter(app);
                app.focus = Focus::Browsing;
                select_row(&mut app.list_state, app.results.len(), app.areas.results, mouse.row);
                true
            } else if app.areas.queue.contains(pos.into()) {
                clear_favorites_filter(app);
                app.focus = Focus::Queue;
                select_row(&mut app.queue_state, app.queue.len(), app.areas.queue, mouse.row);
                true
            } else if app.areas.favorites.contains(pos.into()) {
                app.focus = Focus::Favorites;
                let visible_len = app.visible_favorites().len();
                select_row(
                    &mut app.favorites_state,
                    visible_len,
                    app.areas.favorites,
                    mouse.row,
                );
                true
            } else {
                false
            }
        }
        // Moved/Drag/etc.: sin efecto. Crossterm activa el modo «any event»
        // y repintar en cada movimiento sería una tormenta de renders.
        _ => false,
    }
}

/// Click en la gauge de progreso: la fracción del ancho interno es la
/// fracción de la duración a la que saltar.
fn seek_from_bar_click(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    column: u16,
) {
    if app.queue.current.is_none() {
        return;
    }
    let Some(duration) = app.status.duration else {
        return;
    };
    let area = app.areas.player;
    let inner = area.width.saturating_sub(2) as f64;
    if inner <= 0.0 {
        return;
    }
    let offset = (column.saturating_sub(area.x + 1)) as f64;
    let ratio = (offset / inner).clamp(0.0, 1.0);
    seek_to(
        app,
        player,
        tx,
        Duration::from_secs_f64(ratio * duration.as_secs_f64()),
    );
}

/// Selecciona la fila visible de una lista a partir de la fila del terminal
/// donde cayó el click: offset de scroll + distancia al borde superior.
/// Fuera de las filas con contenido deja la selección como está.
fn select_row(state: &mut ratatui::widgets::ListState, len: usize, area: Rect, row: u16) {
    if len == 0 || row <= area.y || row >= area.y + area.height.saturating_sub(1) {
        return;
    }
    let visible = state.offset() + usize::from(row - area.y - 1);
    if visible < len {
        state.select(Some(visible));
    }
}

pub(super) fn handle_key(
    app: &mut App,
    key: KeyEvent,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return;
    }
    // El overlay de playlists es modal: captura todas las teclas salvo Ctrl+C.
    if !matches!(app.overlay, Overlay::Closed) {
        handle_overlay_key(app, key, player, tx);
        return;
    }
    match app.focus {
        Focus::Searching => match key.code {
            KeyCode::Enter => submit_search(app, tx),
            KeyCode::Esc | KeyCode::Tab => {
                app.history_reset();
                app.focus = Focus::Browsing;
            }
            KeyCode::BackTab => {
                app.history_reset();
                app.focus = Focus::Favorites;
            }
            KeyCode::Up => app.history_prev(),
            KeyCode::Down => app.history_next(),
            KeyCode::Right => app.accept_suggestion(),
            KeyCode::Backspace => {
                app.input.pop();
            }
            KeyCode::Char(c) => app.input.push(c),
            _ => {}
        },
        Focus::Browsing => match key.code {
            KeyCode::Tab => app.focus = app.focus.next(),
            KeyCode::BackTab => app.focus = app.focus.prev(),
            KeyCode::Enter => {
                if let Some(index) = app.selected() {
                    play_result(app, player, tx, index);
                }
            }
            KeyCode::Char('a') => {
                if let Some(index) = app.selected() {
                    add_to_queue(app, app.results[index].clone());
                }
            }
            KeyCode::Char('e') => {
                if let Some(index) = app.selected() {
                    add_next_to_queue(app, app.results[index].clone());
                }
            }
            KeyCode::Char('f') => {
                let track = app
                    .selected()
                    .and_then(|i| app.results.get(i))
                    .or_else(|| app.queue.current_track());
                toggle_favorite(app, track.cloned());
            }
            KeyCode::Char('A') => {
                let track = app
                    .selected()
                    .and_then(|i| app.results.get(i))
                    .or_else(|| app.queue.current_track());
                open_pick_playlist(app, track.cloned());
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(index) = app.selected() {
                    app.select(index.saturating_sub(1));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(index) = app.selected() {
                    app.select(index + 1);
                }
            }
            KeyCode::PageUp => {
                if let Some(index) = app.selected() {
                    app.select(index.saturating_sub(app.list_page_size));
                }
            }
            KeyCode::PageDown => {
                if let Some(index) = app.selected() {
                    app.select(index.saturating_add(app.list_page_size));
                }
            }
            KeyCode::Char('g') => app.select(0),
            KeyCode::Char('G') => app.select(usize::MAX),
            _ => playback_key(app, key, player, tx),
        },
        Focus::Queue => match key.code {
            KeyCode::Tab => app.focus = app.focus.next(),
            KeyCode::BackTab => app.focus = app.focus.prev(),
            KeyCode::Esc => app.focus = Focus::Browsing,
            KeyCode::Enter => {
                if let Some(index) = app.queue_selected() {
                    play_queue_track(app, player, tx, index);
                }
            }
            KeyCode::Char('d') => remove_from_queue(app, player, tx),
            KeyCode::Char('w') => {
                if app.queue.is_empty() {
                    app.message = Some(Message::info("la cola está vacía"));
                } else {
                    app.overlay_input = format!("cola {}", crate::stats::today_string());
                    app.overlay = Overlay::NameInput(NameContext::SaveQueue);
                }
            }
            KeyCode::Char('e') => {
                if let Some(index) = app.queue_selected() {
                    let track = app.queue.tracks[index].clone();
                    let new_index = add_next_to_queue(app, track);
                    app.queue_select(new_index);
                }
            }
            KeyCode::Char('f') => {
                let track = app
                    .queue_selected()
                    .and_then(|i| app.queue.tracks.get(i))
                    .or_else(|| app.queue.current_track());
                toggle_favorite(app, track.cloned());
            }
            KeyCode::Char('A') => {
                let track = app
                    .queue_selected()
                    .and_then(|i| app.queue.tracks.get(i))
                    .or_else(|| app.queue.current_track());
                open_pick_playlist(app, track.cloned());
            }
            KeyCode::Char('J') => {
                if let Some(index) = app.queue_selected()
                    && app.queue.move_down(index)
                {
                    app.queue_select(index + 1);
                }
            }
            KeyCode::Char('K') => {
                if let Some(index) = app.queue_selected()
                    && app.queue.move_up(index)
                {
                    app.queue_select(index - 1);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(index) = app.queue_selected() {
                    app.queue_select(index.saturating_sub(1));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(index) = app.queue_selected() {
                    app.queue_select(index + 1);
                }
            }
            KeyCode::PageUp => {
                if let Some(index) = app.queue_selected() {
                    app.queue_select(index.saturating_sub(app.list_page_size));
                }
            }
            KeyCode::PageDown => {
                if let Some(index) = app.queue_selected() {
                    app.queue_select(index.saturating_add(app.list_page_size));
                }
            }
            KeyCode::Char('g') => app.queue_select(0),
            KeyCode::Char('G') => app.queue_select(usize::MAX),
            _ => playback_key(app, key, player, tx),
        },
        Focus::Favorites => {
            // Con el filtro en edición las teclas alimentan el filtro, no la
            // lista (mismo patrón que la caja de búsqueda principal).
            if matches!(app.favorites_filter_state, FilterState::Editing) {
                filter_edit_key(
                    &mut app.favorites_filter,
                    &mut app.favorites_filter_state,
                    key,
                );
                app.favorites_select(0);
                return;
            }
            match key.code {
            KeyCode::Tab => {
                app.favorites_filter.clear();
                app.favorites_filter_state = FilterState::Off;
                app.focus = app.focus.next();
            }
            KeyCode::BackTab => {
                app.favorites_filter.clear();
                app.favorites_filter_state = FilterState::Off;
                app.focus = app.focus.prev();
            }
            KeyCode::Esc => {
                // Esc suelta el filtro si estaba aplicado; si no, vuelve a
                // resultados como siempre.
                if matches!(app.favorites_filter_state, FilterState::Applied) {
                    app.favorites_filter.clear();
                    app.favorites_filter_state = FilterState::Off;
                    app.favorites_select(0);
                } else {
                    app.focus = Focus::Browsing;
                }
            }
            KeyCode::Char('/') => {
                app.favorites_filter.clear();
                app.favorites_filter_state = FilterState::Editing;
            }
            KeyCode::Enter => {
                if let Some(track) = app
                    .favorites_selected()
                    .and_then(|index| app.favorite_at(index))
                {
                    app.queue.prune_played();
                    let queue_index = app.queue.add(track);
                    play_queue_track(app, player, tx, queue_index);
                }
            }
            KeyCode::Char('a') => {
                if let Some(track) = app
                    .favorites_selected()
                    .and_then(|index| app.favorite_at(index))
                {
                    add_to_queue(app, track);
                }
            }
            KeyCode::Char('e') => {
                if let Some(track) = app
                    .favorites_selected()
                    .and_then(|index| app.favorite_at(index))
                {
                    add_next_to_queue(app, track);
                }
            }
            KeyCode::Char('d') | KeyCode::Char('f') => {
                if let Some(visible) = app.favorites_selected()
                    && let Some(real) = app.favorite_real_index(visible)
                    && let Some(track) = app.favorites.remove(real)
                {
                    app.message =
                        Some(Message::info(format!("quitada de favoritos: {}", track.title)));
                    app.save_favorites();
                    if app.favorites.tracks.is_empty() {
                        app.favorites_state.select(None);
                    } else {
                        app.favorites_select(visible);
                    }
                }
            }
            KeyCode::Char('A') => {
                let track = app
                    .favorites_selected()
                    .and_then(|i| app.favorite_at(i))
                    .or_else(|| app.queue.current_track().cloned());
                open_pick_playlist(app, track);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(index) = app.favorites_selected() {
                    app.favorites_select(index.saturating_sub(1));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(index) = app.favorites_selected() {
                    app.favorites_select(index + 1);
                }
            }
            KeyCode::PageUp => {
                if let Some(index) = app.favorites_selected() {
                    app.favorites_select(index.saturating_sub(app.list_page_size));
                }
            }
            KeyCode::PageDown => {
                if let Some(index) = app.favorites_selected() {
                    app.favorites_select(index.saturating_add(app.list_page_size));
                }
            }
            KeyCode::Char('g') => app.favorites_select(0),
            KeyCode::Char('G') => app.favorites_select(usize::MAX),
            _ => playback_key(app, key, player, tx),
            }
        }
    }
}

/// Paleta de comandos (`:`): edición como los otros inputs de overlay; Enter
/// ejecuta el comando cuyo prefijo más largo coincida con la entrada.
fn overlay_command_key(
    app: &mut App,
    key: KeyEvent,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
    match key.code {
        KeyCode::Esc => app.overlay = Overlay::Closed,
        KeyCode::Enter => {
            let input = app.overlay_input.clone();
            if !super::commands::run(app, player, tx, &input) {
                app.message = Some(Message::error(format!(
                    "comando desconocido: «{input}» — escribe : para ver los disponibles"
                )));
            }
            app.overlay = Overlay::Closed;
        }
        KeyCode::Backspace => {
            app.overlay_input.pop();
        }
        KeyCode::Char(c) => app.overlay_input.push(c),
        _ => {}
    }
}

/// Edición del filtro en vivo (favoritos y contenido de playlist): los
/// caracteres alimentan el texto; Enter pasa a «aplicado», Esc lo suelta.
/// Con el filtro vacío ambos salen a `Off` (lista completa).
fn filter_edit_key(filter: &mut String, state: &mut FilterState, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            *state = if filter.is_empty() {
                FilterState::Off
            } else {
                FilterState::Applied
            };
        }
        KeyCode::Esc => {
            filter.clear();
            *state = FilterState::Off;
        }
        KeyCode::Backspace => {
            filter.pop();
        }
        KeyCode::Char(c) => filter.push(c),
        _ => {}
    }
}

/// El filtro de favoritos es estado del foco: salir de él lo limpia.
fn clear_favorites_filter(app: &mut App) {
    app.favorites_filter.clear();
    app.favorites_filter_state = FilterState::Off;
}

/// Teclas de reproducción compartidas por los focos Browsing y Queue.
fn playback_key(
    app: &mut App,
    key: KeyEvent,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('/') => app.focus = Focus::Searching,
        KeyCode::Char(' ') => spawn_player(player, tx, |p| async move { p.toggle_pause().await }),
        KeyCode::Char('s') => {
            app.queue.current = None;
            app.radio_epoch += 1;
            app.playback_active = false;
            app.mpv_appended = None;
            app.message = Some(Message::info("detenido"));
            spawn_player(player, tx, |p| async move { p.stop().await });
        }
        KeyCode::Char('+') | KeyCode::Char('=') => change_volume(app, player, tx, VOLUME_STEP),
        KeyCode::Char('-') => change_volume(app, player, tx, -VOLUME_STEP),
        KeyCode::Left => seek_by(app, player, tx, -seek_step(key)),
        KeyCode::Right => seek_by(app, player, tx, seek_step(key)),
        KeyCode::Char('l') => app.next_repeat_mode(),
        KeyCode::Char('S') => shuffle_queue(app),
        KeyCode::Char('y') => copy_track(app),
        KeyCode::Char('?') => app.overlay = Overlay::Help,
        KeyCode::Char(':') => {
            // Paleta de comandos (focos de lista; en la búsqueda `:` sigue
            // siendo texto, como cualquier otro carácter).
            app.overlay_input.clear();
            app.overlay = Overlay::Command;
        }
        KeyCode::Char('n') => play_adjacent(app, player, tx, Queue::next_index, "fin de la cola"),
        KeyCode::Char('p') => play_adjacent(app, player, tx, Queue::prev_index, "inicio de la cola"),
        KeyCode::Char('P') => {
            app.overlay = Overlay::List;
            app.pending_delete = None;
            if app.overlay_list_state.selected().is_none() {
                app.overlay_list_select(0);
            }
        }
        KeyCode::Char('m') => app.next_provider(),
        KeyCode::Char('L') => {
            let track = current_or_selected(app);
            open_lyrics(app, track, tx);
        }
        KeyCode::Char('i') => {
            if app.queue.current_track().is_some() {
                app.overlay = Overlay::NowPlaying;
                request_cover(app, tx);
            } else {
                app.message = Some(Message::info("nada en reproducción"));
            }
        }
        KeyCode::Char('t') => app.open_stats(),
        KeyCode::Char('X') => {
            app.overlay = Overlay::Export;
            app.overlay_list_state.select(Some(0));
        }
        KeyCode::Char('I') => {
            app.overlay_input.clear();
            app.overlay = Overlay::PathInput(PathAction::Import);
        }
        KeyCode::Char('r') => {
            app.toggle_radio();
            // Si suena la última pista de la cola, prefetch inmediato.
            radio_prefetch(app, tx);
        }
        KeyCode::Char('R') => app.next_radio_mode(),
        _ => {}
    }
}

/// Paso de seek según el modificador: ±5 s, con Shift ±30 s.
fn seek_step(key: KeyEvent) -> i32 {
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        SEEK_STEP_LARGE
    } else {
        SEEK_STEP
    }
}

fn open_pick_playlist(app: &mut App, track: Option<Track>) {
    match track {
        Some(track) => {
            app.overlay = Overlay::Pick(track);
            // La lista incluye "nueva playlist…" aunque no haya playlists.
            app.overlay_list_state.select(Some(0));
        }
        None => app.message = Some(Message::info("nada que añadir a una playlist")),
    }
}

fn handle_overlay_key(
    app: &mut App,
    key: KeyEvent,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
    let overlay = app.overlay.clone();
    match overlay {
        Overlay::Closed => {}
        Overlay::List => overlay_list_key(app, key),
        Overlay::Content(playlist) => overlay_content_key(app, key, player, tx, playlist),
        Overlay::NameInput(context) => overlay_name_key(app, key, context),
        Overlay::Pick(track) => overlay_pick_key(app, key, track),
        Overlay::Help => match key.code {
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Enter => app.overlay = Overlay::Closed,
            _ => {}
        },
        Overlay::Stats => overlay_stats_key(app, key, player, tx),
        Overlay::Export => overlay_export_key(app, key),
        Overlay::PathInput(action) => overlay_path_key(app, key, &action),
        Overlay::Command => overlay_command_key(app, key, player, tx),
        Overlay::Lyrics => overlay_lyrics_key(app, key),
        Overlay::NowPlaying => match key.code {
            KeyCode::Esc | KeyCode::Char('i') => app.overlay = Overlay::Closed,
            _ => {}
        },
    }
}

/// Pista para la letra: la que suena; si no, la seleccionada según el foco.
fn current_or_selected(app: &App) -> Option<Track> {
    if let Some(track) = app.queue.current_track() {
        return Some(track.clone());
    }
    match app.focus {
        Focus::Browsing => app.selected().and_then(|i| app.results.get(i)).cloned(),
        Focus::Queue => app
            .queue_selected()
            .and_then(|i| app.queue.tracks.get(i))
            .cloned(),
        Focus::Favorites => app
            .favorites_selected()
            .and_then(|i| app.favorite_at(i)),
        Focus::Searching => None,
    }
}

/// Copia al portapapeles la pista que suena o la seleccionada: título,
/// artista y URL de origen.
fn copy_track(app: &mut App) {
    let Some(track) = current_or_selected(app) else {
        app.message = Some(Message::info("nada que copiar"));
        return;
    };
    let text = format!("{} — {}\n{}", track.title, track.artist, track.source_url);
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(clipboard) => clipboard,
        Err(err) => {
            app.message = Some(Message::error(format!("portapapeles no disponible: {err}")));
            return;
        }
    };
    match clipboard.set_text(&text) {
        Ok(()) => app.message = Some(Message::info(format!(
            "copiado al portapapeles: {}",
            track.source_url
        ))),
        Err(err) => app.message = Some(Message::error(format!("no se pudo copiar: {err}"))),
    }
}

/// Scroll del overlay de letras.
fn overlay_lyrics_key(app: &mut App, key: KeyEvent) {
    let lines = app
        .lyrics_text
        .as_ref()
        .map_or(1, |t| t.lines().count() as u16);
    let max_scroll = lines.saturating_sub(app.overlay_page_size as u16);
    match key.code {
        KeyCode::Esc | KeyCode::Char('L') => app.overlay = Overlay::Closed,
        KeyCode::Up | KeyCode::Char('k') => {
            app.lyrics_scroll = app.lyrics_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.lyrics_scroll = (app.lyrics_scroll + 1).min(max_scroll);
        }
        KeyCode::PageUp => {
            app.lyrics_scroll = app.lyrics_scroll.saturating_sub(app.overlay_page_size as u16);
        }
        KeyCode::PageDown => {
            app.lyrics_scroll =
                (app.lyrics_scroll + app.overlay_page_size as u16).min(max_scroll);
        }
        KeyCode::Char('g') => app.lyrics_scroll = 0,
        KeyCode::Char('G') => app.lyrics_scroll = max_scroll,
        _ => {}
    }
}

/// Overlay de estadísticas: pistas más escuchadas / artistas más oídos.
fn overlay_stats_key(
    app: &mut App,
    key: KeyEvent,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
    let len = match app.stats_view {
        StatsView::Artists => app.stats.top_artists().len(),
        StatsView::Top => app.stats_top.len(),
        StatsView::History => app.stats.history().len(),
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('t') => app.overlay = Overlay::Closed,
        KeyCode::Tab => {
            app.stats_view = app.stats_view.next();
            app.stats_state.select(if len > 0 { Some(0) } else { None });
        }
        KeyCode::Enter => {
            if !matches!(app.stats_view, StatsView::Artists)
                && let Some(track) = stats_selected_track(app)
                && !track.source_url.is_empty()
            {
                app.overlay = Overlay::Closed;
                app.queue.prune_played();
                let queue_index = app.queue.add(track);
                play_queue_track(app, player, tx, queue_index);
            }
        }
        KeyCode::Char('a') => {
            if !matches!(app.stats_view, StatsView::Artists)
                && let Some(track) = stats_selected_track(app)
                && !track.source_url.is_empty()
            {
                add_to_queue(app, track);
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(index) = app.stats_state.selected() {
                app.stats_select(index.saturating_sub(1));
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(index) = app.stats_state.selected() {
                app.stats_select(index + 1);
            }
        }
        KeyCode::PageUp => {
            if let Some(index) = app.stats_state.selected() {
                app.stats_select(index.saturating_sub(app.overlay_page_size));
            }
        }
        KeyCode::PageDown => {
            if let Some(index) = app.stats_state.selected() {
                app.stats_select(index.saturating_add(app.overlay_page_size));
            }
        }
        KeyCode::Char('g') => app.stats_select(0),
        KeyCode::Char('G') => app.stats_select(usize::MAX),
        _ => {}
    }
}

/// Pista seleccionada en el overlay de estadísticas según la vista activa:
/// la cache del top en «pistas», la entrada convertida en «historial».
/// En «artistas» no hay pista (la lista agrupa por artista).
fn stats_selected_track(app: &App) -> Option<Track> {
    let index = app.stats_state.selected()?;
    match app.stats_view {
        StatsView::Artists => None,
        StatsView::Top => app.stats_top.get(index).cloned(),
        StatsView::History => app
            .stats
            .history()
            .get(index)
            .map(|entry| entry.to_track()),
    }
}

/// Menú de exportación JSON: elige el ámbito y pide la ruta.
fn overlay_export_key(app: &mut App, key: KeyEvent) {
    use crate::portable::ExportScope;
    const SCOPES: [ExportScope; 3] = [
        ExportScope::All,
        ExportScope::Playlists,
        ExportScope::Favorites,
    ];
    match key.code {
        KeyCode::Esc | KeyCode::Char('X') => app.overlay = Overlay::Closed,
        KeyCode::Enter => {
            if let Some(index) = app.overlay_list_state.selected()
                && let Some(scope) = SCOPES.get(index)
            {
                app.overlay_input = "~/shellmusic-backup.json".to_string();
                app.overlay = Overlay::PathInput(PathAction::ExportJson(*scope));
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(index) = app.overlay_list_state.selected() {
                app.overlay_list_state.select(Some(index.saturating_sub(1)));
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(index) = app.overlay_list_state.selected() {
                app.overlay_list_state
                    .select(Some((index + 1).min(SCOPES.len() - 1)));
            }
        }
        _ => {}
    }
}

/// Input de ruta para exportar o importar (edición como overlay_name_key).
fn overlay_path_key(app: &mut App, key: KeyEvent, action: &PathAction) {
    match key.code {
        KeyCode::Esc => {
            app.overlay = match action {
                PathAction::ExportJson(_) => Overlay::Export,
                PathAction::ExportM3u(playlist) => Overlay::Content(*playlist),
                PathAction::Import => Overlay::Closed,
            };
        }
        KeyCode::Enter => app.run_path_action(action),
        KeyCode::Backspace => {
            app.overlay_input.pop();
        }
        KeyCode::Char(c) => app.overlay_input.push(c),
        _ => {}
    }
}

fn overlay_list_key(app: &mut App, key: KeyEvent) {
    if key.code != KeyCode::Char('d') {
        app.pending_delete = None;
    }
    match key.code {
        KeyCode::Esc | KeyCode::Char('P') => app.overlay = Overlay::Closed,
        KeyCode::Char('n') => {
            app.overlay_input.clear();
            app.overlay = Overlay::NameInput(NameContext::Plain);
        }
        KeyCode::Char('d') => {
            if let Some(index) = app.overlay_list_state.selected() {
                if app.pending_delete == Some(index) {
                    if let Some(playlist) = app.playlists.delete(index) {
                        app.message =
                            Some(Message::info(format!("playlist «{}» eliminada", playlist.name)));
                        app.save_playlists();
                    }
                    app.pending_delete = None;
                    if app.playlists.items.is_empty() {
                        app.overlay_list_state.select(None);
                    } else {
                        app.overlay_list_select(index);
                    }
                } else if let Some(playlist) = app.playlists.items.get(index) {
                    app.pending_delete = Some(index);
                    app.message = Some(Message::info(format!(
                        "pulsa d de nuevo para eliminar «{}»",
                        playlist.name
                    )));
                }
            }
        }
        KeyCode::Enter => {
            if let Some(index) = app.overlay_list_state.selected()
                && index < app.playlists.items.len()
            {
                // Visita nueva: el filtro de contenido arranca limpio.
                app.content_filter.clear();
                app.content_filter_state = FilterState::Off;
                app.overlay = Overlay::Content(index);
                app.overlay_content_select(index, 0);
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(index) = app.overlay_list_state.selected() {
                app.overlay_list_select(index.saturating_sub(1));
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(index) = app.overlay_list_state.selected() {
                app.overlay_list_select(index + 1);
            }
        }
        KeyCode::PageUp => {
            if let Some(index) = app.overlay_list_state.selected() {
                app.overlay_list_select(index.saturating_sub(app.overlay_page_size));
            }
        }
        KeyCode::PageDown => {
            if let Some(index) = app.overlay_list_state.selected() {
                app.overlay_list_select(index.saturating_add(app.overlay_page_size));
            }
        }
        KeyCode::Char('g') => app.overlay_list_select(0),
        KeyCode::Char('G') => app.overlay_list_select(usize::MAX),
        _ => {}
    }
}

fn overlay_content_key(
    app: &mut App,
    key: KeyEvent,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    playlist: usize,
) {
    // Edición del filtro: las teclas alimentan el texto (como en favoritos).
    if matches!(app.content_filter_state, FilterState::Editing) {
        filter_edit_key(&mut app.content_filter, &mut app.content_filter_state, key);
        app.content_select(playlist, 0);
        return;
    }
    let selected = app.overlay_content_state.selected();
    match key.code {
        KeyCode::Esc => {
            if matches!(app.content_filter_state, FilterState::Applied) {
                app.content_filter.clear();
                app.content_filter_state = FilterState::Off;
                app.content_select(playlist, 0);
            } else {
                app.overlay = Overlay::List;
            }
        }
        KeyCode::Char('/') => {
            app.content_filter.clear();
            app.content_filter_state = FilterState::Editing;
        }
        KeyCode::Enter => {
            if let Some(index) = selected
                && let Some(track) = app.content_at(playlist, index)
            {
                // El overlay permanece abierto para seguir gestionando la playlist.
                app.queue.prune_played();
                let queue_index = app.queue.add(track);
                play_queue_track(app, player, tx, queue_index);
            }
        }
        KeyCode::Char('a') => {
            if let Some(index) = selected
                && let Some(track) = app.content_at(playlist, index)
            {
                add_to_queue(app, track);
            }
        }
        // Reproducir la playlist entera: encola todo y suena la primera.
        KeyCode::Char('A') => {
            if let Some(tracks) = app.playlists.items.get(playlist).map(|p| p.tracks.clone())
                && !tracks.is_empty()
            {
                let name = app.playlists.items[playlist].name.clone();
                app.queue.prune_played();
                let mut first = None;
                for track in tracks {
                    let index = app.queue.add(track);
                    first.get_or_insert(index);
                }
                if let Some(first) = first {
                    app.overlay = Overlay::Closed;
                    app.message =
                        Some(Message::info(format!("reproduciendo playlist «{name}»")));
                    play_queue_track(app, player, tx, first);
                }
            }
        }
        KeyCode::Char('d') => {
            if let Some(visible) = selected
                && let Some(real) = app.content_real_index(playlist, visible)
                && let Some(track) = app.playlists.remove_track(playlist, real)
            {
                app.message = Some(Message::info(format!(
                    "quitada de la playlist: {}",
                    track.title
                )));
                app.save_playlists();
                app.content_select(playlist, visible);
            }
        }
        // Exportar esta playlist a M3U.
        KeyCode::Char('X') => {
            if let Some(pl) = app.playlists.items.get(playlist) {
                app.overlay_input =
                    format!("~/{}.m3u", crate::portable::sanitize_filename(&pl.name));
                app.overlay = Overlay::PathInput(PathAction::ExportM3u(playlist));
            }
        }
        KeyCode::Char('J') => {
            if let Some(visible) = selected
                && let Some(real) = app.content_real_index(playlist, visible)
                && app.playlists.move_track_down(playlist, real)
            {
                // Tras mover, la pista sigue siendo la misma: reanclar la
                // selección visible requiere saber dónde quedó en la lista
                // filtrada; lo más simple es mantener la posición visible.
                app.content_select(playlist, visible);
                app.save_playlists();
            }
        }
        KeyCode::Char('K') => {
            if let Some(visible) = selected
                && let Some(real) = app.content_real_index(playlist, visible)
                && app.playlists.move_track_up(playlist, real)
            {
                app.content_select(playlist, visible);
                app.save_playlists();
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(index) = selected {
                app.content_select(playlist, index.saturating_sub(1));
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(index) = selected {
                app.content_select(playlist, index + 1);
            }
        }
        KeyCode::PageUp => {
            if let Some(index) = selected {
                app.content_select(
                    playlist,
                    index.saturating_sub(app.overlay_page_size),
                );
            }
        }
        KeyCode::PageDown => {
            if let Some(index) = selected {
                app.content_select(
                    playlist,
                    index.saturating_add(app.overlay_page_size),
                );
            }
        }
        KeyCode::Char('g') => app.content_select(playlist, 0),
        KeyCode::Char('G') => app.content_select(playlist, usize::MAX),
        _ => {}
    }
}

/// Crea una playlist con el contenido actual de la cola (en orden).
fn save_queue_as_playlist(app: &mut App, name: &str) {
    if app.queue.is_empty() {
        app.message = Some(Message::info("la cola está vacía"));
        app.overlay = Overlay::Closed;
        return;
    }
    match app.playlists.create(name) {
        Ok(index) => {
            let tracks: Vec<Track> = app.queue.tracks.clone();
            for track in tracks {
                app.playlists.add_track(index, track);
            }
            app.save_playlists();
            let created = app.playlists.items[index].name.clone();
            let count = app.playlists.items[index].tracks.len();
            app.message = Some(Message::info(format!(
                "cola guardada como playlist «{created}» ({count} pistas)"
            )));
            app.overlay = Overlay::Closed;
        }
        Err(err) => app.message = Some(Message::error(err)),
    }
}

fn overlay_name_key(app: &mut App, key: KeyEvent, context: NameContext) {
    match key.code {
        KeyCode::Esc => {
            app.overlay = match context {
                NameContext::Plain => Overlay::List,
                NameContext::WithTrack(track) => Overlay::Pick(*track),
                // Viene del foco Cola, no del gestor de playlists.
                NameContext::SaveQueue => Overlay::Closed,
            };
        }
        KeyCode::Enter => {
            let name = app.overlay_input.clone();
            match context {
                NameContext::SaveQueue => {
                    save_queue_as_playlist(app, &name);
                }
                NameContext::Plain | NameContext::WithTrack(_) => {
                    match app.playlists.create(&name) {
                        Ok(index) => {
                            let created = app.playlists.items[index].name.clone();
                            app.save_playlists();
                            match context {
                                NameContext::Plain => {
                                    app.message =
                                        Some(Message::info(format!("playlist «{created}» creada")));
                                    app.overlay = Overlay::List;
                                    app.overlay_list_select(index);
                                }
                                NameContext::WithTrack(track) => {
                                    app.playlists.add_track(index, *track.clone());
                                    app.save_playlists();
                                    app.message = Some(Message::info(format!(
                                        "añadida a «{created}»: {}",
                                        track.title
                                    )));
                                    app.overlay = Overlay::Closed;
                                }
                                NameContext::SaveQueue => unreachable!("caso ya manejado"),
                            }
                        }
                        Err(err) => app.message = Some(Message::error(err)),
                    }
                }
            }
        }
        KeyCode::Backspace => {
            app.overlay_input.pop();
        }
        KeyCode::Char(c) => app.overlay_input.push(c),
        _ => {}
    }
}

fn overlay_pick_key(app: &mut App, key: KeyEvent, track: Track) {
    let count = app.playlists.items.len(); // +1 por la entrada "nueva playlist…"
    match key.code {
        KeyCode::Esc => app.overlay = Overlay::Closed,
        KeyCode::Enter => {
            if let Some(index) = app.overlay_list_state.selected() {
                if index >= count {
                    app.overlay_input.clear();
                    app.overlay = Overlay::NameInput(NameContext::WithTrack(Box::new(track)));
                } else {
                    let name = app.playlists.items[index].name.clone();
                    if app.playlists.add_track(index, track.clone()) {
                        app.message =
                            Some(Message::info(format!("añadida a «{name}»: {}", track.title)));
                    } else {
                        app.message =
                            Some(Message::info(format!("ya está en «{name}»: {}", track.title)));
                    }
                    app.save_playlists();
                    app.overlay = Overlay::Closed;
                }
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(index) = app.overlay_list_state.selected() {
                app.overlay_list_state.select(Some(index.saturating_sub(1)));
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(index) = app.overlay_list_state.selected() {
                app.overlay_list_state
                    .select(Some((index + 1).min(count))); // count = índice de "nueva playlist…"
            }
        }
        KeyCode::PageUp => {
            if let Some(index) = app.overlay_list_state.selected() {
                app.overlay_list_state
                    .select(Some(index.saturating_sub(app.overlay_page_size)));
            }
        }
        KeyCode::PageDown => {
            if let Some(index) = app.overlay_list_state.selected() {
                app.overlay_list_state.select(Some(
                    index.saturating_add(app.overlay_page_size).min(count),
                ));
            }
        }
        KeyCode::Char('g') => app.overlay_list_state.select(Some(0)),
        KeyCode::Char('G') => app.overlay_list_state.select(Some(count)),
        _ => {}
    }
}
