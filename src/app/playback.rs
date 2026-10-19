use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;

use crate::player::Player;
use crate::providers::Track;
use crate::queue::{Queue, Removed};

use super::AppMsg;
use super::state::{App, Focus, Message};

pub(super) fn handle_msg(app: &mut App, msg: AppMsg) {
    match msg {
        AppMsg::SearchDone(Ok(tracks)) => {
            app.searching = false;
            if tracks.is_empty() {
                app.message = Some(Message::info("sin resultados"));
            } else {
                app.message = None;
                app.results = tracks;
                app.select(0);
                app.focus = Focus::Browsing;
            }
        }
        AppMsg::SearchDone(Err(err)) => {
            app.searching = false;
            app.message = Some(Message::error(err));
        }
        AppMsg::Error(err) => app.message = Some(Message::error(err)),
        AppMsg::PlaybackStarted => {
            if app.message.as_ref().is_some_and(|m| !m.is_error) {
                app.message = None;
            }
        }
    }
}

pub(super) fn add_to_queue(app: &mut App, track: Track) {
    app.queue.add(track.clone());
    app.message = Some(Message::info(format!("añadida a la cola: {}", track.title)));
    if app.queue_selected().is_none() {
        app.queue_select(0);
    }
}

pub(super) fn toggle_favorite(app: &mut App, track: Option<Track>) {
    let Some(track) = track else {
        app.message = Some(Message::info("nada que marcar como favorito"));
        return;
    };
    if app.favorites.toggle(&track) {
        app.message = Some(Message::info(format!("★ añadida a favoritos: {}", track.title)));
        if app.favorites_selected().is_none() {
            app.favorites_select(0);
        }
    } else {
        app.message = Some(Message::info(format!("quitada de favoritos: {}", track.title)));
    }
    app.save_favorites();
}

pub(super) fn submit_search(app: &mut App, tx: &mpsc::UnboundedSender<AppMsg>) {
    let query = app.input.trim().to_string();
    if query.is_empty() || app.searching {
        return;
    }
    let Some((_, provider)) = app.providers.get(app.active_provider).cloned() else {
        return;
    };
    app.searching = true;
    app.message = Some(Message::info(format!("buscando «{query}»…")));
    let tx = tx.clone();
    let limit = app.results_limit;
    tokio::spawn(async move {
        let result = provider.search(&query, limit).await.map_err(|e| e.to_string());
        let _ = tx.send(AppMsg::SearchDone(result));
    });
}

/// Enter en Resultados: añade a la cola si falta y reproduce al instante.
pub(super) fn play_result(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    result_index: usize,
) {
    if result_index >= app.results.len() {
        return;
    }
    let track = app.results[result_index].clone();
    let index = app.queue.add(track);
    play_queue_track(app, player, tx, index);
}

pub(super) fn play_queue_track(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    index: usize,
) {
    if index >= app.queue.len() {
        return;
    }
    app.queue.current = Some(index);
    app.queue_select(index);
    app.message = Some(Message::info(format!(
        "cargando «{}»…",
        app.queue.tracks[index].title
    )));
    let url = app.queue.tracks[index].source_url.clone();
    let player = Arc::clone(player);
    let tx = tx.clone();
    tokio::spawn(async move {
        match player.play(&url).await {
            Ok(()) => {
                let _ = tx.send(AppMsg::PlaybackStarted);
            }
            Err(err) => {
                let _ = tx.send(AppMsg::Error(format!("no se pudo reproducir: {err}")));
            }
        }
    });
}

pub(super) fn play_adjacent(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    neighbor: fn(&Queue) -> Option<usize>,
    at_edge: &str,
) {
    if app.queue.is_empty() {
        app.message = Some(Message::info("la cola está vacía"));
        return;
    }
    match neighbor(&app.queue) {
        Some(index) => play_queue_track(app, player, tx, index),
        None => app.message = Some(Message::info(at_edge)),
    }
}

pub(super) fn autoplay_next(app: &mut App, player: &Arc<dyn Player>, tx: &mpsc::UnboundedSender<AppMsg>) {
    if let Some(index) = app.queue.next_index()
        && app.queue.current.is_some()
    {
        play_queue_track(app, player, tx, index);
    }
}

pub(super) fn remove_from_queue(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
    let Some(index) = app.queue_selected() else {
        return;
    };
    match app.queue.remove(index) {
        Some(Removed::CurrentWithSuccessor(successor)) => {
            app.message = Some(Message::info("eliminada; saltando a la siguiente"));
            play_queue_track(app, player, tx, successor);
        }
        Some(Removed::CurrentWithoutSuccessor) => {
            app.message = Some(Message::info("eliminada la pista que sonaba; detenido"));
            spawn_player(player, tx, |p| async move { p.stop().await });
        }
        Some(Removed::Other) => {
            app.message = Some(Message::info("eliminada de la cola"));
        }
        None => return,
    }
    if app.queue.is_empty() {
        app.queue_state.select(None);
    } else {
        app.queue_select(index);
    }
}

pub(super) fn change_volume(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    delta: i16,
) {
    let volume = (i16::from(app.status.volume) + delta).clamp(0, 100) as u8;
    app.status.volume = volume;
    app.message = Some(Message::info(format!("volumen: {volume}")));
    let player = Arc::clone(player);
    let tx = tx.clone();
    tokio::spawn(async move {
        if let Err(err) = player.set_volume(volume).await {
            let _ = tx.send(AppMsg::Error(format!("no se pudo ajustar el volumen: {err}")));
        }
    });
}

pub(super) fn spawn_player<F, Fut>(
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    action: F,
) where
    F: FnOnce(Arc<dyn Player>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<()>> + Send,
{
    let player = Arc::clone(player);
    let tx = tx.clone();
    tokio::spawn(async move {
        if let Err(err) = action(player).await {
            let _ = tx.send(AppMsg::Error(format!("error del reproductor: {err}")));
        }
    });
}
