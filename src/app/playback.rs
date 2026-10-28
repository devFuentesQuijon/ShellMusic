use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::mpsc;

use crate::player::Player;
use crate::providers::{MusicProvider, ProviderKind, Track, provider_for_url};
use crate::queue::{Queue, Removed};

use super::AppMsg;
use super::state::{App, ControlAction, Focus, Message, RadioMode, RepeatMode};

/// Libera la petición de control pendiente si una búsqueda más reciente (de
/// la UI o de otra petición) dejó su epoch obsoleto: el cliente recibe error
/// en lugar de quedarse esperando un resultado que nunca llegará. La llama
/// `handle_msg` y el tick periódico del bucle principal.
pub(super) fn release_stale_control_pending(app: &mut App) {
    let stale = app
        .control_pending
        .as_ref()
        .is_some_and(|pending| pending.epoch != app.search_epoch);
    if stale
        && let Some(pending) = app.control_pending.take()
    {
        let _ = pending
            .reply
            .send(crate::control::fail("la búsqueda fue reemplazada por otra más reciente"));
    }
}

pub(super) fn handle_msg(
    app: &mut App,
    msg: AppMsg,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
    release_stale_control_pending(app);
    match msg {
        AppMsg::SearchDone(epoch, Ok(tracks)) => {
            // Resultado de una búsqueda anterior: no sobrescribe la actual.
            if epoch != app.search_epoch {
                return;
            }
            app.searching = false;
            if tracks.is_empty() {
                app.autoplay_first = false;
                app.message = Some(Message::info("sin resultados"));
                if let Some(pending) = app.control_pending.take() {
                    let _ = pending
                        .reply
                        .send(crate::control::ok(serde_json::json!({ "results": [] })));
                }
            } else {
                app.message = None;
                app.results = tracks;
                app.select(0);
                app.focus = Focus::Browsing;
                // `--play`: la primera coincidencia suena sola.
                if app.autoplay_first {
                    app.autoplay_first = false;
                    play_result(app, player, tx, 0);
                }
                // Petición de control (MCP) esperando este resultado.
                if let Some(pending) = app.control_pending.take() {
                    let response = match pending.action {
                        ControlAction::QueueFirst => {
                            let track = app.results[0].clone();
                            app.queue.add(track.clone());
                            crate::control::ok(serde_json::json!({
                                "queued": crate::control::track_json(&track)
                            }))
                        }
                        ControlAction::ReplyResults => crate::control::ok(
                            serde_json::json!({ "results": crate::control::tracks_json(&app.results) }),
                        ),
                        ControlAction::PlayFirst => {
                            // autoplay_first ya reprodujo la primera (arriba).
                            crate::control::ok(serde_json::json!({
                                "playing": crate::control::track_json(&app.results[0])
                            }))
                        }
                    };
                    let _ = pending.reply.send(response);
                }
            }
        }
        AppMsg::SearchDone(epoch, Err(err)) => {
            if epoch != app.search_epoch {
                return;
            }
            app.searching = false;
            app.autoplay_first = false;
            app.message = Some(Message::error(err.clone()));
            if let Some(pending) = app.control_pending.take() {
                let _ = pending.reply.send(crate::control::fail(err));
            }
        }
        AppMsg::Error(err) => app.message = Some(Message::error(err)),
        AppMsg::LyricsDone(result) => {
            app.lyrics_text = Some(match result {
                Ok(text) => text,
                Err(err) => format!("No se pudo cargar la letra: {err}"),
            });
            app.lyrics_scroll = 0;
        }
        AppMsg::CoverDone(Ok(cover)) => {
            // PNG para la notificación con carátula (best-effort: si el
            // disco falla, la notificación irá sin imagen).
            save_notification_cover(&app.cover_png_path, &cover.image);
            let key = cover.track;
            app.cover_ready_for = Some(key.clone());
            if let Some(picker) = &app.picker {
                app.cover = Some(picker.new_resize_protocol(cover.image));
                app.cover_for = Some(key);
            }
        }
        AppMsg::CoverDone(Err(err)) => {
            // Sin carátula el panel funciona igual: el fallo solo resuelve
            // la notificación pendiente (se manda sin imagen).
            app.cover_failed_for = Some(err.track);
        }
        AppMsg::PlaybackStarted => {
            if app.message.as_ref().is_some_and(|m| !m.is_error) {
                app.message = None;
            }
            confirm_track_start(app, tx);
            gapless_append_next(app, player, tx);
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

/// «Encolar a continuación»: inserta la pista justo detrás de la actual.
/// Devuelve el índice final de la pista en la cola.
pub(super) fn add_next_to_queue(app: &mut App, track: Track) -> usize {
    app.queue.prune_played();
    let index = app.queue.insert_next(track.clone());
    app.message = Some(Message::info(format!("a continuación: {}", track.title)));
    if app.queue_selected().is_none() {
        app.queue_select(0);
    }
    index
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
    // Una URL de cualquier proveedor conocido se resuelve con ese proveedor,
    // aunque no sea el activo; si no está habilitado, se usa el activo.
    let provider = provider_for_url(&query)
        .and_then(|kind| {
            app.providers
                .iter()
                .find(|(k, _)| *k == kind)
                .map(|(_, p)| Arc::clone(p))
        })
        .or_else(|| {
            app.providers
                .get(app.active_provider)
                .map(|(_, p)| Arc::clone(p))
        });
    let Some(provider) = provider else {
        return;
    };
    app.history.push(&query);
    app.history_reset();
    app.save_history();
    app.searching = true;
    app.search_epoch += 1;
    let epoch = app.search_epoch;
    app.message = Some(Message::info(format!("buscando «{query}»…")));
    let tx = tx.clone();
    let limit = app.results_limit;
    tokio::spawn(async move {
        let result = provider.search(&query, limit).await.map_err(|e| e.to_string());
        let _ = tx.send(AppMsg::SearchDone(epoch, result));
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
    // Un loadfile con replace (el modo por defecto) limpia la playlist
    // interna de mpv: la pista apilada para gapless ya no existe.
    app.mpv_appended = None;
    // Marca el inicio tentativo: se confirma en PlaybackStarted. El
    // scrobble de la pista anterior ocurre ahí (no aquí: aún no sabemos si
    // esta carga arrancará).
    mark_track_start(app);
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
    // Estadísticas de escucha: cada arranque de pista cuenta. El guardado se
    // difiere al periodic_save (cada ~30 s): grabar en cada cambio de pista
    // era una escritura síncrona a disco en la ruta caliente.
    app.stats.record(&app.queue.tracks[index]);
    app.stats_dirty = true;
    app.queue_select(index);
    app.message = Some(Message::info(format!(
        "cargando «{}»…",
        app.queue.tracks[index].title
    )));
    // Persiste la cola al cambiar de pista: sobrevive a cierres abruptos.
    app.save_session(start);
    // Carátula para la notificación con imagen (deduped internamente): se
    // pide siempre al cambiar de pista, no solo al abrir el panel «i».
    request_cover(app, tx);
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
        app.mpv_appended = None;
        play_queue_track(app, player, tx, app.queue.current.unwrap());
        return;
    }
    if let Some(index) = app.queue.next_index() {
        // Gapless: si la pista que mpv ya está sonando es la que toca, solo
        // se avanza el estado; la reproducción no se interrumpe. Si la cola
        // cambió entre medias (borrado/movido/barajado), la clave no cuadra
        // y se recarga con replace como siempre.
        let appended_matches = app
            .mpv_appended
            .as_ref()
            .zip(app.queue.tracks.get(index))
            .is_some_and(|(key, track)| key.0 == track.provider && key.1 == track.id);
        if app.gapless && appended_matches {
            gapless_advance(app, index);
            gapless_append_next(app, player, tx);
            // Carátula de la nueva pista para la notificación con imagen.
            request_cover(app, tx);
            radio_prefetch(app, tx);
            return;
        }
        app.mpv_appended = None;
        play_queue_track(app, player, tx, index);
        return;
    }
    if app.repeat == RepeatMode::All && !app.queue.is_empty() {
        app.mpv_appended = None;
        play_queue_track(app, player, tx, 0);
        return;
    }
    if app.radio {
        // Si el prefetch de la pista que acaba de terminar sigue en vuelo,
        // no duplica la búsqueda: RadioDone arrancará la reproducción.
        radio_prefetch(app, tx);
    }
}

/// Avance gapless: la pista `index` ya está sonando en mpv (apilada con
/// `append` antes de que terminara la anterior). Replica el bookkeeping de
/// `play_queue_track_from` sin tocar el reproductor.
fn gapless_advance(app: &mut App, index: usize) {
    // La pista anterior ya sonó todo lo que iba a sonar: scrobble si toca.
    scrobble_leaving_standalone(app);
    app.queue.current = Some(index);
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
    app.stats.record(&app.queue.tracks[index]);
    app.stats_dirty = true;
    app.queue_select(index);
    // Gapless: mpv ya la está reproduciendo, el inicio es ahora mismo.
    app.playing = Some((
        app.queue.tracks[index].clone(),
        std::time::SystemTime::now(),
    ));
    app.playing_peak = Duration::ZERO;
    app.save_session(None);
}

/// Scrobble sin canal de mensajes (para gapless, que no lo tiene a mano):
/// los errores de sesión se dejan pasar silenciosos aquí; el siguiente
/// confirm_track_start con tx los reportará si persisten.
fn scrobble_leaving_standalone(app: &mut App) {
    if app.lastfm.is_none() {
        return;
    }
    // Reutiliza la lógica con un canal al que nadie escucha: los errores de
    // re-autenticación se pierden en este camino, aceptado (ver docstring).
    let (tx, _rx) = mpsc::unbounded_channel::<AppMsg>();
    scrobble_leaving(app, &tx);
}

/// Gapless: apila en mpv la siguiente pista de la cola (`loadfile append`)
/// para que al terminar la actual pase a ella sin pausa. Guarda su clave en
/// `mpv_appended`; el avance solo se confía si al terminar la clave encaja
/// con la pista que toca. Con repetición «one» no se apila: la siguiente
/// «reproducción» es la misma pista y se recarga con replace.
fn gapless_append_next(app: &mut App, player: &Arc<dyn Player>, tx: &mpsc::UnboundedSender<AppMsg>) {
    app.mpv_appended = None;
    if !app.gapless || app.repeat == RepeatMode::One {
        return;
    }
    let Some(next) = app.queue.next_index() else {
        return;
    };
    let Some(track) = app.queue.tracks.get(next) else {
        return;
    };
    if track.source_url.is_empty() {
        return;
    }
    let key = (track.provider, track.id.clone());
    let url = track.source_url.clone();
    let player = Arc::clone(player);
    let tx = tx.clone();
    app.mpv_appended = Some(key);
    tokio::spawn(async move {
        if let Err(err) = player.append(&url).await {
            let _ = tx.send(AppMsg::Error(format!(
                "no se pudo apilar la siguiente pista: {err}"
            )));
        }
    });
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

/// Seek a una posición absoluta (click en la barra de progreso, MPRIS
/// SetPosition): se expresa como salto relativo a la posición conocida.
pub(super) fn seek_to(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    target: std::time::Duration,
) {
    if let Some(current) = app.status.position {
        let delta = target.as_secs_f64() - current.as_secs_f64();
        seek_by(app, player, tx, delta as i32);
    }
}

/// Reglas oficiales de Last.fm: la pista debe durar más de 30 s y haberse
/// oído `min(30 s, mitad de la duración)` o más.
pub(crate) fn scrobble_eligible(peak: Duration, dur: Duration) -> bool {
    const MIN_LISTEN: Duration = Duration::from_secs(30);
    dur > MIN_LISTEN && peak >= std::cmp::min(MIN_LISTEN, dur / 2)
}

/// Marca el inicio (tentativo) de la pista que se va a cargar. Se confirma
/// al llegar `PlaybackStarted` (mpv realmente arrancó) y ahí se reinicia el
/// peak de escucha y se avisa a Last.fm de «now playing».
pub(super) fn mark_track_start(app: &mut App) {
    app.pending_start = app
        .queue
        .current_track()
        .map(|track| (track.clone(), std::time::SystemTime::now()));
}

/// Llegó la confirmación de mpv: la pista anterior (si la hay) ya terminó de
/// sonar para efectos de scrobble; la nueva arranca ahora.
pub(super) fn confirm_track_start(app: &mut App, tx: &mpsc::UnboundedSender<AppMsg>) {
    scrobble_leaving(app, tx);
    app.playing = app.pending_start.take();
    app.playing_peak = Duration::ZERO;
    if let (Some(client), Some((track, _))) = (app.lastfm.clone(), &app.playing) {
        let track = track.clone();
        tokio::spawn(async move {
            // nowPlaying es informativo: errores no molestan en la UI.
            let _ = client.now_playing(&track).await;
        });
    }
}

/// Scrobble de la pista que deja de sonar (si cumple las reglas). Se llama
/// en cada punto de salida: cambio de pista, stop y cierre de la app.
pub(super) fn scrobble_leaving(app: &mut App, tx: &mpsc::UnboundedSender<AppMsg>) {
    let (Some(client), Some((track, started))) =
        (app.lastfm.clone(), app.playing.clone())
    else {
        return;
    };
    let Ok(elapsed) = started.elapsed() else {
        return;
    };
    // No scrobblar reinicios inmediatos ni datos absurdos.
    if elapsed < Duration::from_secs(3) {
        return;
    }
    let dur = track
        .duration
        .or(app.status.duration)
        .unwrap_or(Duration::ZERO);
    if !scrobble_eligible(app.playing_peak, dur) {
        return;
    }
    let Ok(timestamp) = started.duration_since(std::time::UNIX_EPOCH) else {
        return;
    };
    let tx = tx.clone();
    tokio::spawn(async move {
        if let Err(err) = client.scrobble(&track, timestamp.as_secs()).await {
            // Solo el error de sesión/clave merece interrumpir al usuario.
            if err.contains("re-autentica") {
                let _ = tx.send(AppMsg::Error(err));
            }
        }
    });
    app.playing = None;
    app.playing_peak = Duration::ZERO;
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

/// Stop completo (tecla s, comando `stop`, MPRIS Stop): scrobblea lo
/// escuchado, limpia el estado y para el reproductor.
pub(super) fn stop_playback(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
    scrobble_leaving(app, tx);
    app.queue.current = None;
    app.radio_epoch += 1;
    app.playback_active = false;
    app.mpv_appended = None;
    app.playing = None;
    app.playing_peak = Duration::ZERO;
    app.pending_start = None;
    app.message = Some(Message::info("detenido"));
    spawn_player(player, tx, |p| async move { p.stop().await });
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

/// Descarga la carátula de la pista actual (miniatura pública de YouTube)
/// si no está ya cacheada. SoundCloud no tiene equivalente sin API: el
/// panel funciona sin imagen.
pub(super) fn request_cover(app: &mut App, tx: &mpsc::UnboundedSender<AppMsg>) {
    let Some(track) = app.queue.current_track() else {
        return;
    };
    if track.provider != ProviderKind::Youtube {
        return;
    }
    let key = (track.provider, track.id.clone());
    // Dedup amplio: vale la imagen en memoria (panel) o el PNG ya en disco
    // (notificación); en ambos casos no hace falta volver a descargar.
    if app.cover_for.as_ref() == Some(&key) || app.cover_ready_for.as_ref() == Some(&key) {
        return;
    }
    let url = format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", track.id);
    let tx = tx.clone();
    tokio::spawn(async move {
        let result = async {
            let bytes = reqwest::Client::builder()
                .user_agent(concat!("ShellMusic/", env!("CARGO_PKG_VERSION")))
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .map_err(|err| err.to_string())?
                .get(&url)
                .send()
                .await
                .map_err(|err| err.to_string())?
                .bytes()
                .await
                .map_err(|err| err.to_string())?;
            image::load_from_memory(&bytes).map_err(|err| err.to_string())
        }
        .await;
        let _ = tx.send(AppMsg::CoverDone(result.map(|image| super::CoverImage {
            track: key.clone(),
            image,
        }).map_err(|message| super::CoverError {
            message,
            track: key,
        })));
    });
}

/// Escribe la carátula como PNG para la notificación con imagen
/// (`Hint::ImagePath`). Best-effort: un fallo de disco solo deja la
/// notificación sin imagen.
fn save_notification_cover(path: &std::path::Path, dynamic: &image::DynamicImage) {
    let mut buffer = std::io::Cursor::new(Vec::new());
    if dynamic
        .write_to(&mut buffer, image::ImageFormat::Png)
        .is_ok()
    {
        let _ = std::fs::write(path, buffer.into_inner());
    }
}

/// Tecla L: overlay de letras para la pista dada (o la que suena).
pub(super) fn open_lyrics(app: &mut App, track: Option<Track>, tx: &mpsc::UnboundedSender<AppMsg>) {
    let Some(track) = track else {
        app.message = Some(Message::info("nada que buscar letra"));
        return;
    };
    app.lyrics_title = format!("{} — {}", track.title, track.artist);
    app.lyrics_text = None;
    app.lyrics_scroll = 0;
    app.overlay = super::state::Overlay::Lyrics;
    let tx = tx.clone();
    tokio::spawn(async move {
        let result = crate::lyrics::fetch_lyrics(&track).await;
        let _ = tx.send(AppMsg::LyricsDone(result));
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

    #[test]
    fn scrobble_rules_match_lastfm_policy() {
        let s = Duration::from_secs;
        // Pista de 200 s: oírla 30 s basta (min(30, 100)).
        assert!(scrobble_eligible(s(30), s(200)));
        assert!(!scrobble_eligible(s(29), s(200)));
        // Pista corta (20 s): nunca scrobble (debe durar >30 s).
        assert!(!scrobble_eligible(s(20), s(20)));
        // Pista de 40 s: hace falta la mitad (20 s), no 30.
        assert!(scrobble_eligible(s(20), s(40)));
        assert!(!scrobble_eligible(s(19), s(40)));
        // Duración desconocida (0): no scrobble.
        assert!(!scrobble_eligible(s(60), Duration::ZERO));
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
    async fn search_done_with_autoplay_first_plays_top_result() {
        let mut app = app_with_provider("autoplay");
        app.autoplay_first = true;
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();
        let epoch = app.search_epoch;

        handle_msg(
            &mut app,
            AppMsg::SearchDone(epoch, Ok(vec![track("x"), track("y")])),
            &player,
            &tx,
        );
        assert_eq!(app.queue.current, Some(0), "suena la primera coincidencia");
        assert_eq!(app.queue.tracks[0].id, "x");
        assert!(!app.autoplay_first, "el flag se consume");

        // Sin el flag no auto-reproduce: el usuario decide con Enter.
        handle_msg(
            &mut app,
            AppMsg::SearchDone(epoch, Ok(vec![track("z")])),
            &player,
            &tx,
        );
        assert_eq!(app.queue.len(), 1, "la segunda búsqueda no auto-reproduce");
        assert_eq!(app.results[0].id, "z");
        assert!(app.queue.current.is_some(), "la reproducción sigue en la pista x");
    }

    #[tokio::test]
    async fn search_done_empty_results_clears_autoplay_flag() {
        let mut app = app_with_provider("autoplay-vacio");
        app.autoplay_first = true;
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();
        let epoch = app.search_epoch;

        handle_msg(
            &mut app,
            AppMsg::SearchDone(epoch, Ok(vec![])),
            &player,
            &tx,
        );
        assert!(!app.autoplay_first, "sin resultados el flag también se consume");
        assert!(app.queue.is_empty());
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
