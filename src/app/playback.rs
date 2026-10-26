use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;

use crate::player::Player;
use crate::providers::{MusicProvider, ProviderKind, Track};
use crate::queue::{Queue, Removed};

use super::AppMsg;
use super::state::{App, Focus, Message, RadioMode, RepeatMode};

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
            // Títulos ya ocupados: los de la cola más los ya reproducidos,
            // para no encolar otra versión de la misma canción.
            let mut occupied: HashSet<String> = app
                .queue
                .tracks
                .iter()
                .map(|t| normalize_title(&t.title, &t.artist))
                .collect();
            occupied.extend(app.played_titles.iter().cloned());
            let candidates =
                radio_candidates(&tracks, &app.queue, &app.played, &occupied, RADIO_TRACKS);
            if candidates.is_empty() {
                app.radio_expanded_for = None;
                app.message = Some(Message::info(format!(
                    "radio: sin pistas nuevas de «{artist}»; fin de la cola"
                )));
                return;
            }
            let count = candidates.len();
            // Poda antes de encolar la tanda: los índices capturados en el
            // bucle se mantienen válidos.
            app.queue.prune_played();
            let mut first = None;
            for track in candidates {
                let index = app.queue.add(track);
                first.get_or_insert(index);
            }
            if let Some(first) = first {
                app.message = Some(Message::info(format!(
                    "Radio: {count} pistas relacionadas con {artist}"
                )));
                // Prefetch con la última pista aún sonando: solo encolar.
                // Cola agotada (reproductor parado): arrancar la primera.
                if !app.playback_active {
                    play_queue_track(app, player, tx, first);
                }
            }
        }
        AppMsg::RadioDone(artist, Err(err), epoch) => {
            if !app.radio || epoch != app.radio_epoch {
                return;
            }
            // Permite reintentar la expansión al agotarse la cola.
            app.radio_expanded_for = None;
            app.message = Some(Message::error(format!(
                "radio: búsqueda de «{artist}» fallida: {err}"
            )));
        }
    }
}

pub(super) fn add_to_queue(app: &mut App, track: Track) {
    app.queue.prune_played();
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
    app.history.push(&query);
    app.history_reset();
    app.save_history();
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
    app.queue.prune_played();
    let index = app.queue.add(track);
    play_queue_track(app, player, tx, index);
}

pub(super) fn play_queue_track(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    index: usize,
) {
    play_queue_track_from(app, player, tx, index, None);
}

/// Reproduce la pista de la cola; `start` reanuda desde esa posición.
pub(super) fn play_queue_track_from(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    index: usize,
    start: Option<std::time::Duration>,
) {
    if index >= app.queue.len() {
        return;
    }
    app.queue.current = Some(index);
    // Invalida cualquier expansión de radio en vuelo: hay reproducción nueva.
    app.radio_epoch += 1;
    app.playback_active = true;
    app.played.insert((
        app.queue.tracks[index].provider,
        app.queue.tracks[index].id.clone(),
    ));
    app.played_titles.insert(normalize_title(
        &app.queue.tracks[index].title,
        &app.queue.tracks[index].artist,
    ));
    // Estadísticas de escucha: cada arranque de pista cuenta.
    app.stats.record(&app.queue.tracks[index]);
    app.refresh_stats_view();
    app.save_stats();
    app.queue_select(index);
    app.message = Some(Message::info(format!(
        "cargando «{}»…",
        app.queue.tracks[index].title
    )));
    // Persiste la cola al cambiar de pista: sobrevive a cierres abruptos.
    app.save_session(start);
    let url = app.queue.tracks[index].source_url.clone();
    let player = Arc::clone(player);
    let play_tx = tx.clone();
    tokio::spawn(async move {
        match player.play(&url, start).await {
            Ok(()) => {
                let _ = play_tx.send(AppMsg::PlaybackStarted);
            }
            Err(err) => {
                let _ = play_tx.send(AppMsg::Error(format!("no se pudo reproducir: {err}")));
            }
        }
    });
    // Si esta es la última pista de la cola, pide ya la próxima tanda de la
    // radio para que esté encolada antes de que termine.
    radio_prefetch(app, tx);
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
    app.playback_active = false;
    if app.repeat == RepeatMode::One {
        play_queue_track(app, player, tx, app.queue.current.unwrap());
        return;
    }
    if let Some(index) = app.queue.next_index() {
        play_queue_track(app, player, tx, index);
        return;
    }
    if app.repeat == RepeatMode::All && !app.queue.is_empty() {
        play_queue_track(app, player, tx, 0);
        return;
    }
    if app.radio {
        // Si el prefetch de la pista que acaba de terminar sigue en vuelo,
        // no duplica la búsqueda: RadioDone arrancará la reproducción.
        radio_prefetch(app, tx);
    }
}

/// ←/→: salto relativo dentro de la pista en reproducción.
pub(super) fn seek_by(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    seconds: i32,
) {
    if app.queue.current.is_none() {
        app.message = Some(Message::info("nada en reproducción"));
        return;
    }
    app.message = Some(Message::info(format!("seek {seconds:+} s")));
    spawn_player(player, tx, move |p| async move { p.seek(seconds).await });
}

/// S: baraja la cola manteniendo la pista que suena en su sitio.
pub(super) fn shuffle_queue(app: &mut App) {
    if app.queue.len() < 2 {
        app.message = Some(Message::info("cola demasiado corta para barajar"));
        return;
    }
    app.queue.shuffle();
    app.message = Some(Message::info("cola barajada"));
}

const RADIO_TRACKS: usize = 5;
const RAREZAS_SKIP_HITS: usize = 15;

/// Palabras clave para el modo random por género/década.
const RANDOM_GENERO_KEYWORDS: &[&str] = &[
    "80s rock",
    "90s pop hits",
    "jazz classics",
    "lofi hip hop",
    "70s disco",
    "classical piano",
    "reggae roots",
    "synthwave",
    "bossa nova",
    "90s hip hop",
    "80s heavy metal",
    "flamenco",
    "cumbia",
    "ambient",
    "70s funk",
    "2000s indie rock",
];

/// Lanza la expansión de radio si la pista actual es la última de la cola y
/// aún no se pidió tanda para ella. Se usa tanto al empezar la última pista
/// (prefetch) como al agotarse la cola sin prefetch previo (red de seguridad).
pub(super) fn radio_prefetch(app: &mut App, tx: &mpsc::UnboundedSender<AppMsg>) {
    if !app.radio || app.repeat != RepeatMode::Off {
        return;
    }
    let Some(current) = app.queue.current_track() else {
        return;
    };
    if app.queue.next_index().is_some() {
        return;
    }
    let id = (current.provider, current.id.clone());
    if app.radio_expanded_for.as_ref() == Some(&id) {
        return;
    }
    app.radio_expanded_for = Some(id);
    radio_expand(app, tx);
}

fn radio_expand(app: &mut App, tx: &mpsc::UnboundedSender<AppMsg>) {
    let Some(track) = app.queue.current_track().cloned() else {
        return;
    };
    let Some((_, provider)) = app.providers.get(app.active_provider).cloned() else {
        return;
    };
    let epoch = app.radio_epoch;
    let limit = app.results_limit;

    match app.radio_mode {
        RadioMode::Artista => {
            let artist = track.artist.clone();
            app.message = Some(Message::info(format!("radio: buscando pistas de «{artist}»…")));
            spawn_radio_search(provider, artist.clone(), artist, limit, 0, false, epoch, tx);
        }
        RadioMode::Similares => {
            if track.provider == ProviderKind::Youtube {
                // El mix RD de YouTube da similares de verdad; si falla,
                // fallback a búsqueda del artista dentro de la misma tarea.
                app.message = Some(Message::info(format!(
                    "radio: buscando similares a «{}»…",
                    track.title
                )));
                let label = track.artist.clone();
                let video_id = track.id.clone();
                let tx = tx.clone();
                tokio::spawn(async move {
                    let result = match crate::providers::ytdlp::youtube_mix(&video_id).await {
                        Ok(tracks) => Ok(tracks),
                        Err(_) => provider
                            .search(&label, limit)
                            .await
                            .map_err(|e| e.to_string()),
                    };
                    let _ = tx.send(AppMsg::RadioDone(label, result, epoch));
                });
            } else {
                // SoundCloud no tiene mix vía yt-dlp: búsqueda artista+título.
                let query = format!("{} {}", track.artist, track.title);
                app.message = Some(Message::info(format!(
                    "radio: buscando similares a «{}»…",
                    track.title
                )));
                spawn_radio_search(provider, track.artist.clone(), query, limit, 0, false, epoch, tx);
            }
        }
        RadioMode::Random => {
            // Artista al azar de los favoritos; sin favoritos, género al azar.
            let mut artists: Vec<String> = app
                .favorites
                .tracks
                .iter()
                .map(|t| t.artist.clone())
                .collect();
            artists.sort();
            artists.dedup();
            if artists.is_empty() {
                radio_expand_random_genero(app, provider, epoch, limit, tx);
            } else {
                let artist = artists[crate::random::below(artists.len())].clone();
                app.message = Some(Message::info(format!(
                    "radio: descubrimiento desde «{artist}»…"
                )));
                spawn_radio_search(provider, artist.clone(), artist, limit, 0, true, epoch, tx);
            }
        }
        RadioMode::RandomGenero => {
            radio_expand_random_genero(app, provider, epoch, limit, tx);
        }
        RadioMode::Rarezas => {
            let artist = track.artist.clone();
            app.message = Some(Message::info(format!(
                "radio: buscando rarezas de «{artist}»…"
            )));
            // Pide el tope y descarta los primeros hits; baraja el resto.
            spawn_radio_search(provider, artist.clone(), artist, 50, RAREZAS_SKIP_HITS, true, epoch, tx);
        }
    }
}

fn radio_expand_random_genero(
    app: &mut App,
    provider: Arc<dyn MusicProvider>,
    epoch: u64,
    limit: usize,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
    let keyword = RANDOM_GENERO_KEYWORDS[crate::random::below(RANDOM_GENERO_KEYWORDS.len())];
    app.message = Some(Message::info(format!("radio: azar «{keyword}»…")));
    spawn_radio_search(provider, keyword.to_string(), keyword.to_string(), limit, 0, true, epoch, tx);
}

#[allow(clippy::too_many_arguments)]
fn spawn_radio_search(
    provider: Arc<dyn MusicProvider>,
    label: String,
    query: String,
    limit: usize,
    skip_hits: usize,
    shuffle: bool,
    epoch: u64,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
    let tx = tx.clone();
    tokio::spawn(async move {
        let result = provider
            .search(&query, limit)
            .await
            .map_err(|e| e.to_string())
            .map(|mut tracks| {
                if skip_hits > 0 && tracks.len() > skip_hits {
                    tracks.drain(..skip_hits);
                }
                if shuffle {
                    crate::random::shuffle(&mut tracks);
                }
                tracks
            });
        let _ = tx.send(AppMsg::RadioDone(label, result, epoch));
    });
}

/// Normaliza un título para deduplicar versiones de la misma canción en la
/// radio: minúsculas, sin segmentos entre () ni [], sin el nombre del
/// artista, solo alfanumérico y espacios colapsados. Si queda vacío, se usa
/// el título original en minúsculas para no colapsar títulos raros.
fn normalize_title(title: &str, artist: &str) -> String {
    let mut stripped = String::with_capacity(title.len());
    let mut depth = 0u32;
    for c in title.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => stripped.push(c),
            _ => {}
        }
    }
    let mut normalized = stripped.to_lowercase();
    let artist_lower = artist.to_lowercase();
    if artist_lower.chars().count() > 2 {
        normalized = normalized.replace(&artist_lower, " ");
    }
    let normalized: String = normalized
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.is_empty() {
        title.to_lowercase()
    } else {
        normalized
    }
}

fn radio_candidates(
    results: &[Track],
    queue: &Queue,
    played: &HashSet<(ProviderKind, String)>,
    occupied_titles: &HashSet<String>,
    max: usize,
) -> Vec<Track> {
    let mut seen = occupied_titles.clone();
    results
        .iter()
        .filter(|track| {
            !played.contains(&(track.provider, track.id.clone())) && queue.contains(track).is_none()
        })
        // seen.insert devuelve false si el título normalizado ya salió:
        // deduplica contra la cola/reproducidas y dentro de la propia tanda.
        .filter(|track| seen.insert(normalize_title(&track.title, &track.artist)))
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
        let candidates = radio_candidates(&results, &queue, &played, &HashSet::new(), 5);
        let ids: Vec<&str> = candidates.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["nueva-1", "nueva-2"]);
    }

    #[test]
    fn radio_candidates_respects_max_and_order() {
        let queue = Queue::new();
        let played = HashSet::new();
        let results: Vec<Track> = (0..8).map(|i| track(&i.to_string())).collect();
        let candidates = radio_candidates(&results, &queue, &played, &HashSet::new(), 5);
        let ids: Vec<&str> = candidates.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["0", "1", "2", "3", "4"]);
    }

    #[test]
    fn normalize_title_strips_versions_and_artist() {
        assert_eq!(
            normalize_title("Enjoy the Silence", "Depeche Mode"),
            "enjoy the silence"
        );
        assert_eq!(
            normalize_title("Enjoy the Silence (2006 Remaster)", "Depeche Mode"),
            "enjoy the silence"
        );
        assert_eq!(
            normalize_title("Depeche Mode - Enjoy The Silence [Official Video]", "Depeche Mode"),
            "enjoy the silence"
        );
        // Títulos distintos no colapsan
        assert_ne!(
            normalize_title("Enjoy the Silence", "Depeche Mode"),
            normalize_title("Personal Jesus", "Depeche Mode")
        );
        // Normalización vacía: cae al título original para no colapsar todo
        assert_eq!(normalize_title("(((", "x"), "(((".to_lowercase());
    }

    #[test]
    fn radio_candidates_dedupes_song_versions() {
        let queue = Queue::new();
        let played = HashSet::new();
        let mut enjoy1 = track("v1");
        enjoy1.title = "Enjoy the Silence".to_string();
        enjoy1.artist = "Depeche Mode".to_string();
        let mut enjoy2 = track("v2");
        enjoy2.title = "Enjoy the Silence (2006 Remaster)".to_string();
        enjoy2.artist = "Depeche Mode".to_string();
        let mut enjoy3 = track("v3");
        enjoy3.title = "Depeche Mode - Enjoy The Silence [Official Video]".to_string();
        enjoy3.artist = "Depeche Mode".to_string();
        let mut other = track("v4");
        other.title = "Personal Jesus".to_string();
        other.artist = "Depeche Mode".to_string();

        let results = vec![enjoy1, enjoy2, enjoy3, other];
        let candidates = radio_candidates(&results, &queue, &played, &HashSet::new(), 5);
        let ids: Vec<&str> = candidates.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["v1", "v4"], "solo una versión por canción");

        // Y si ya sonó una versión, ninguna otra entra.
        let occupied: HashSet<String> = ["enjoy the silence".to_string()].into_iter().collect();
        let mut r1 = track("v1");
        r1.title = "Enjoy the Silence (Live)".to_string();
        r1.artist = "Depeche Mode".to_string();
        let mut r2 = track("v2");
        r2.title = "Policy of Truth".to_string();
        r2.artist = "Depeche Mode".to_string();
        let candidates = radio_candidates(&[r1, r2], &queue, &played, &occupied, 5);
        let ids: Vec<&str> = candidates.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["v2"]);
    }

    struct MockPlayer;

    #[async_trait::async_trait]
    impl Player for MockPlayer {
        async fn play(&self, _url: &str, _start: Option<std::time::Duration>) -> Result<()> {
            Ok(())
        }
        async fn toggle_pause(&self) -> Result<()> {
            Ok(())
        }
        async fn stop(&self) -> Result<()> {
            Ok(())
        }
        async fn seek(&self, _seconds: i32) -> Result<()> {
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
    async fn autoplay_repeat_one_replays_current() {
        let mut app = App::with_data_dir(
            &crate::config::Config::default(),
            &std::env::temp_dir().join(format!("shellmusic-repeat1-{}", std::process::id())),
        )
        .unwrap();
        app.queue.add(track("a"));
        app.queue.add(track("b"));
        app.queue.current = Some(1);
        app.repeat = RepeatMode::One;
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();

        autoplay_next(&mut app, &player, &tx);
        assert_eq!(app.queue.current, Some(1), "repite la misma pista");
    }

    #[tokio::test]
    async fn autoplay_repeat_all_wraps_to_first() {
        let mut app = App::with_data_dir(
            &crate::config::Config::default(),
            &std::env::temp_dir().join(format!("shellmusic-repeatall-{}", std::process::id())),
        )
        .unwrap();
        app.queue.add(track("a"));
        app.queue.add(track("b"));
        app.queue.current = Some(1); // última de la cola
        app.repeat = RepeatMode::All;
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();

        autoplay_next(&mut app, &player, &tx);
        assert_eq!(app.queue.current, Some(0), "vuelve a la primera");
    }

    #[tokio::test]
    async fn autoplay_repeat_off_stops_at_end() {
        let mut app = App::with_data_dir(
            &crate::config::Config::default(),
            &std::env::temp_dir().join(format!("shellmusic-repeatoff-{}", std::process::id())),
        )
        .unwrap();
        app.queue.add(track("a"));
        app.queue.current = Some(0);
        app.repeat = RepeatMode::Off;
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();

        autoplay_next(&mut app, &player, &tx);
        // Sin repetición ni radio no hay reproducción nueva: la cola queda
        // como estaba y no se encola nada.
        assert_eq!(app.queue.len(), 1);
        assert_eq!(app.queue.current, Some(0));
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

    struct MockProvider;

    #[async_trait::async_trait]
    impl MusicProvider for MockProvider {
        async fn search(&self, _query: &str, _limit: usize) -> Result<Vec<Track>> {
            Ok(Vec::new())
        }
    }

    fn app_with_provider(name: &str) -> App {
        let mut app = App::with_data_dir(
            &crate::config::Config::default(),
            &std::env::temp_dir().join(format!("shellmusic-{name}-{}", std::process::id())),
        )
        .unwrap();
        app.providers = vec![(ProviderKind::Youtube, Arc::new(MockProvider))];
        app
    }

    #[tokio::test]
    async fn radio_done_con_reproduccion_activa_solo_encola() {
        let mut app = app_with_provider("radiodone-activa");
        app.radio = true;
        app.playback_active = true;
        app.queue.add(track("a"));
        app.queue.current = Some(0);
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();

        // Prefetch: la última pista sigue sonando, las nuevas solo se encolan.
        let epoch = app.radio_epoch;
        handle_msg(
            &mut app,
            AppMsg::RadioDone("artista".to_string(), Ok(vec![track("x"), track("y")]), epoch),
            &player,
            &tx,
        );
        assert_eq!(app.queue.len(), 3);
        assert_eq!(app.queue.current, Some(0), "no interrumpe lo que suena");
    }

    #[tokio::test]
    async fn radio_done_error_permite_reintento() {
        let mut app = app_with_provider("radiodone-error");
        app.radio = true;
        app.radio_expanded_for = Some((ProviderKind::Youtube, "a".to_string()));
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();

        let epoch = app.radio_epoch;
        handle_msg(
            &mut app,
            AppMsg::RadioDone("artista".to_string(), Err("fallo".to_string()), epoch),
            &player,
            &tx,
        );
        assert_eq!(app.radio_expanded_for, None, "limpia para reintentar");
    }

    #[tokio::test]
    async fn prefetch_se_lanza_al_empezar_ultima_pista() {
        let mut app = app_with_provider("prefetch-lanza");
        app.radio = true;
        app.repeat = RepeatMode::Off;
        app.radio_mode = RadioMode::Artista;
        app.queue.add(track("a"));
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();

        play_queue_track(&mut app, &player, &tx, 0);
        assert_eq!(
            app.radio_expanded_for,
            Some((ProviderKind::Youtube, "a".to_string()))
        );
        assert!(
            app.message.as_ref().is_some_and(|m| m.text.contains("buscando")),
            "mensaje de búsqueda en curso"
        );

        // Segunda llamada para la misma pista: no relanza la búsqueda.
        app.message = None;
        radio_prefetch(&mut app, &tx);
        assert!(app.message.is_none(), "no duplica la expansión");
    }

    #[tokio::test]
    async fn prefetch_no_se_lanza_con_repeat_all_ni_con_siguiente_en_cola() {
        let mut app = app_with_provider("prefetch-norepeat");
        app.radio = true;
        app.repeat = RepeatMode::All;
        app.radio_mode = RadioMode::Artista;
        app.queue.add(track("a"));
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();

        play_queue_track(&mut app, &player, &tx, 0);
        assert_eq!(app.radio_expanded_for, None, "repeat all no usa radio");

        let mut app = app_with_provider("prefetch-con-siguiente");
        app.radio = true;
        app.repeat = RepeatMode::Off;
        app.radio_mode = RadioMode::Artista;
        app.queue.add(track("a"));
        app.queue.add(track("b"));
        let (tx, _rx) = mpsc::unbounded_channel();

        play_queue_track(&mut app, &player, &tx, 0);
        assert_eq!(
            app.radio_expanded_for, None,
            "hay siguiente en cola: sin prefetch"
        );
    }

    #[tokio::test]
    async fn radio_done_poda_cola_antes_de_encolar() {
        let mut app = app_with_provider("radiodone-poda");
        app.radio = true;
        app.playback_active = true;
        for i in 0..crate::queue::MAX_QUEUE + 10 {
            app.queue.add(track(&format!("t{i}")));
        }
        app.queue.current = Some(crate::queue::MAX_QUEUE + 5);
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();

        let epoch = app.radio_epoch;
        handle_msg(
            &mut app,
            AppMsg::RadioDone("artista".to_string(), Ok(vec![track("x"), track("y")]), epoch),
            &player,
            &tx,
        );
        // Podadas las antiguas (margen de KEEP_BEFORE_CURRENT) + 2 nuevas.
        assert_eq!(
            app.queue.len(),
            crate::queue::KEEP_BEFORE_CURRENT + 5 + 2
        );
        let current = app.queue.current.unwrap();
        assert_eq!(app.queue.tracks[current].id, format!("t{}", crate::queue::MAX_QUEUE + 5));
        assert_eq!(app.queue.next_index(), Some(current + 1));
    }
}
