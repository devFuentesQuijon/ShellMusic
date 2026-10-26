use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

use crate::player::Player;
use crate::providers::Track;
use crate::queue::Queue;

use super::AppMsg;
use super::playback::{
    add_to_queue, change_volume, open_lyrics, play_adjacent, play_queue_track, play_result,
    radio_prefetch, remove_from_queue, request_cover, seek_by, shuffle_queue, spawn_player,
    submit_search, toggle_favorite,
};
use super::state::{App, Focus, Message, NameContext, Overlay, PathAction};

const VOLUME_STEP: i16 = 5;
const SEEK_STEP: i32 = 5;
const SEEK_STEP_LARGE: i32 = 30;

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
        Focus::Favorites => match key.code {
            KeyCode::Tab => app.focus = app.focus.next(),
            KeyCode::BackTab => app.focus = app.focus.prev(),
            KeyCode::Esc => app.focus = Focus::Browsing,
            KeyCode::Enter => {
                if let Some(index) = app.favorites_selected() {
                    let track = app.favorites.tracks[index].clone();
                    app.queue.prune_played();
                    let queue_index = app.queue.add(track);
                    play_queue_track(app, player, tx, queue_index);
                }
            }
            KeyCode::Char('a') => {
                if let Some(index) = app.favorites_selected() {
                    add_to_queue(app, app.favorites.tracks[index].clone());
                }
            }
            KeyCode::Char('d') | KeyCode::Char('f') => {
                if let Some(index) = app.favorites_selected() {
                    if let Some(track) = app.favorites.remove(index) {
                        app.message =
                            Some(Message::info(format!("quitada de favoritos: {}", track.title)));
                        app.save_favorites();
                    }
                    if app.favorites.tracks.is_empty() {
                        app.favorites_state.select(None);
                    } else {
                        app.favorites_select(index);
                    }
                }
            }
            KeyCode::Char('A') => {
                let track = app
                    .favorites_selected()
                    .and_then(|i| app.favorites.tracks.get(i))
                    .or_else(|| app.queue.current_track());
                open_pick_playlist(app, track.cloned());
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
        },
    }
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
            .and_then(|i| app.favorites.tracks.get(i))
            .cloned(),
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
    let len = if app.stats_by_artist {
        app.stats.top_artists().len()
    } else {
        app.stats_top.len()
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('t') => app.overlay = Overlay::Closed,
        KeyCode::Tab => {
            app.stats_by_artist = !app.stats_by_artist;
            app.stats_state.select(if len > 0 { Some(0) } else { None });
        }
        KeyCode::Enter => {
            if !app.stats_by_artist
                && let Some(index) = app.stats_state.selected()
                && let Some(track) = app.stats_top.get(index).cloned()
                && !track.source_url.is_empty()
            {
                app.overlay = Overlay::Closed;
                app.queue.prune_played();
                let queue_index = app.queue.add(track);
                play_queue_track(app, player, tx, queue_index);
            }
        }
        KeyCode::Char('a') => {
            if !app.stats_by_artist
                && let Some(index) = app.stats_state.selected()
                && let Some(track) = app.stats_top.get(index).cloned()
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
    let selected = app.overlay_content_state.selected();
    match key.code {
        KeyCode::Esc => app.overlay = Overlay::List,
        KeyCode::Enter => {
            if let Some(index) = selected
                && let Some(track) = app.playlists.items.get(playlist).and_then(|p| p.tracks.get(index)).cloned()
            {
                // El overlay permanece abierto para seguir gestionando la playlist.
                app.queue.prune_played();
                let queue_index = app.queue.add(track);
                play_queue_track(app, player, tx, queue_index);
            }
        }
        KeyCode::Char('a') => {
            if let Some(index) = selected
                && let Some(track) = app.playlists.items.get(playlist).and_then(|p| p.tracks.get(index)).cloned()
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
            if let Some(index) = selected
                && let Some(track) = app.playlists.remove_track(playlist, index)
            {
                app.message = Some(Message::info(format!(
                    "quitada de la playlist: {}",
                    track.title
                )));
                app.save_playlists();
                app.overlay_content_select(playlist, index);
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
            if let Some(index) = selected
                && app.playlists.move_track_down(playlist, index)
            {
                app.overlay_content_select(playlist, index + 1);
                app.save_playlists();
            }
        }
        KeyCode::Char('K') => {
            if let Some(index) = selected
                && app.playlists.move_track_up(playlist, index)
            {
                app.overlay_content_select(playlist, index - 1);
                app.save_playlists();
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(index) = selected {
                app.overlay_content_select(playlist, index.saturating_sub(1));
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(index) = selected {
                app.overlay_content_select(playlist, index + 1);
            }
        }
        KeyCode::PageUp => {
            if let Some(index) = selected {
                app.overlay_content_select(
                    playlist,
                    index.saturating_sub(app.overlay_page_size),
                );
            }
        }
        KeyCode::PageDown => {
            if let Some(index) = selected {
                app.overlay_content_select(
                    playlist,
                    index.saturating_add(app.overlay_page_size),
                );
            }
        }
        KeyCode::Char('g') => app.overlay_content_select(playlist, 0),
        KeyCode::Char('G') => app.overlay_content_select(playlist, usize::MAX),
        _ => {}
    }
}

fn overlay_name_key(app: &mut App, key: KeyEvent, context: NameContext) {
    match key.code {
        KeyCode::Esc => {
            app.overlay = match context {
                NameContext::Plain => Overlay::List,
                NameContext::WithTrack(track) => Overlay::Pick(*track),
            };
        }
        KeyCode::Enter => {
            let name = app.overlay_input.clone();
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
                    }
                }
                Err(err) => app.message = Some(Message::error(err)),
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
