use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;

use crate::player::Player;
use crate::providers::{ProviderKind, Track};
use crate::queue::{Queue, Removed};

use super::AppMsg;
use super::state::{App, Focus, Message};

pub(super) fn handle_msg(
    app: &mut App,
    msg: AppMsg,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
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
        AppMsg::RadioDone(artist, Ok(tracks), epoch) => {
            // Si el usuario detuvo, reprodujo otra cosa o desactivó la radio
            // mientras buscábamos, la expansión quedó obsoleta y se descarta.
            if !app.radio || epoch != app.radio_epoch {
                return;
            }
            let candidates = radio_candidates(&tracks, &app.queue, &app.played, RADIO_TRACKS);
            if candidates.is_empty() {
                app.message = Some(Message::info(format!(
                    "radio: sin pistas nuevas de «{artist}»; fin de la cola"
                )));
                return;
            }
            let count = candidates.len();
            let mut first = None;
            for track in candidates {
                let index = app.queue.add(track);
                first.get_or_insert(index);
            }
            if let Some(first) = first {
                app.message = Some(Message::info(format!(
                    "Radio: {count} pistas relacionadas con {artist}"
                )));
                play_queue_track(app, player, tx, first);
            }
        }
        AppMsg::RadioDone(artist, Err(err), epoch) => {
            if !app.radio || epoch != app.radio_epoch {
                return;
            }
            app.message = Some(Message::error(format!(
                "radio: búsqueda de «{artist}» fallida: {err}"
            )));
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
    // Invalida cualquier expansión de radio en vuelo: hay reproducción nueva.
    app.radio_epoch += 1;
    app.played.insert((
        app.queue.tracks[index].provider,
        app.queue.tracks[index].id.clone(),
    ));
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
    if app.queue.current.is_none() {
        return;
    }
    if let Some(index) = app.queue.next_index() {
        play_queue_track(app, player, tx, index);
        return;
    }
    if app.radio {
        radio_expand(app, tx);
    }
}

const RADIO_TRACKS: usize = 5;

fn radio_expand(app: &mut App, tx: &mpsc::UnboundedSender<AppMsg>) {
    let Some(artist) = app.queue.current_track().map(|track| track.artist.clone()) else {
        return;
    };
    let Some((_, provider)) = app.providers.get(app.active_provider).cloned() else {
        return;
    };
    app.message = Some(Message::info(format!("radio: buscando pistas de «{artist}»…")));
    let epoch = app.radio_epoch;
    let tx = tx.clone();
    let limit = app.results_limit;
    tokio::spawn(async move {
        let result = provider.search(&artist, limit).await.map_err(|e| e.to_string());
        let _ = tx.send(AppMsg::RadioDone(artist, result, epoch));
    });
}

fn radio_candidates(
    results: &[Track],
    queue: &Queue,
    played: &HashSet<(ProviderKind, String)>,
    max: usize,
) -> Vec<Track> {
    results
        .iter()
        .filter(|track| {
            !played.contains(&(track.provider, track.id.clone())) && queue.contains(track).is_none()
        })
        .take(max)
        .cloned()
        .collect()
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
            app.radio_epoch += 1;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str) -> Track {
        Track {
            id: id.to_string(),
            title: format!("título {id}"),
            artist: "artista".to_string(),
            duration: None,
            source_url: format!("https://example.com/{id}"),
            provider: ProviderKind::Youtube,
        }
    }

    #[test]
    fn radio_candidates_excludes_played_and_queued() {
        let mut queue = Queue::new();
        queue.add(track("en-cola"));
        let played: HashSet<(ProviderKind, String)> =
            [(ProviderKind::Youtube, "vista".to_string())].into_iter().collect();
        let results = vec![
            track("vista"),
            track("en-cola"),
            track("nueva-1"),
            track("nueva-2"),
        ];
        let candidates = radio_candidates(&results, &queue, &played, 5);
        let ids: Vec<&str> = candidates.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["nueva-1", "nueva-2"]);
    }

    #[test]
    fn radio_candidates_respects_max_and_order() {
        let queue = Queue::new();
        let played = HashSet::new();
        let results: Vec<Track> = (0..8).map(|i| track(&i.to_string())).collect();
        let candidates = radio_candidates(&results, &queue, &played, 5);
        let ids: Vec<&str> = candidates.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["0", "1", "2", "3", "4"]);
    }

    struct MockPlayer;

    #[async_trait::async_trait]
    impl Player for MockPlayer {
        async fn play(&self, _url: &str) -> Result<()> {
            Ok(())
        }
        async fn toggle_pause(&self) -> Result<()> {
            Ok(())
        }
        async fn stop(&self) -> Result<()> {
            Ok(())
        }
        async fn set_volume(&self, _volume: u8) -> Result<()> {
            Ok(())
        }
        async fn status(&self) -> Result<crate::player::PlaybackStatus> {
            Ok(crate::player::PlaybackStatus::default())
        }
    }

    #[tokio::test]
    async fn radio_done_with_stale_epoch_is_ignored() {
        let mut app = App::with_data_dir(
            &crate::config::Config::default(),
            &std::env::temp_dir().join(format!("shellmusic-radio-test-{}", std::process::id())),
        )
        .unwrap();
        app.radio = true;
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();
        let results = vec![track("x"), track("y")];

        // El usuario pulsó stop (o reprodujo otra cosa) tras disparar la búsqueda.
        let epoch = app.radio_epoch;
        app.radio_epoch += 1;
        handle_msg(
            &mut app,
            AppMsg::RadioDone("artista".to_string(), Ok(results.clone()), epoch),
            &player,
            &tx,
        );
        assert!(app.queue.is_empty());
        assert_eq!(app.queue.current, None);

        // Con el epoch vigente sí expande y reproduce la primera.
        let epoch = app.radio_epoch;
        handle_msg(
            &mut app,
            AppMsg::RadioDone("artista".to_string(), Ok(results), epoch),
            &player,
            &tx,
        );
        assert_eq!(app.queue.len(), 2);
        assert_eq!(app.queue.current, Some(0));
    }

    #[tokio::test]
    async fn radio_done_after_radio_disabled_is_ignored() {
        let mut app = App::with_data_dir(
            &crate::config::Config::default(),
            &std::env::temp_dir().join(format!("shellmusic-radio2-test-{}", std::process::id())),
        )
        .unwrap();
        app.radio = true;
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();
        let results = vec![track("x"), track("y")];

        // El usuario desactiva la radio con la búsqueda en vuelo: toggle
        // invalida el epoch y el flag, así que el resultado se descarta.
        let epoch = app.radio_epoch;
        app.toggle_radio();
        handle_msg(
            &mut app,
            AppMsg::RadioDone("artista".to_string(), Ok(results), epoch),
            &player,
            &tx,
        );
        assert!(app.queue.is_empty());
        assert_eq!(app.queue.current, None);
    }
}
