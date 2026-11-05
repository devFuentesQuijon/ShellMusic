//! Despacho de peticiones del socket de control (RF-63) sobre el estado de
//! la app. Llamado desde el `select!` principal por cada `ControlRequest`;
//! responde de inmediato salvo `search`/`play`/`queue_add`, cuya respuesta
//! depende de la búsqueda y se envía al llegar el `SearchDone` de su epoch
//! (véase `ControlPending`).

use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::control::{self, ControlRequest};
use crate::player::Player;
use crate::queue::Removed;

use super::commands;
use super::playback::{self, spawn_player};
use super::state::{App, ControlAction, ControlPending};
use super::AppMsg;

/// Ejecuta una petición de control. Consume el `reply` del request: lo usa
/// de inmediato (peticiones síncronas) o lo guarda en `control_pending`
/// (búsquedas), caso en el que la respuesta llega de forma asíncrona.
pub(super) fn handle(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    request: ControlRequest,
) {
    let ControlRequest { body, reply } = request;
    // Option porque solo un arm puede consumirlo: las síncronas responden al
    // momento; las de búsqueda lo guardan en `control_pending` para cuando
    // llegue el resultado.
    let mut reply = Some(reply);
    let Some(req) = body.get("req").and_then(Value::as_str) else {
        respond(&mut reply, control::fail("falta «req»"));
        return;
    };
    match req {
        "now_playing" => respond(&mut reply, now_playing(app)),
        "search" => submit_search(app, tx, &body, &mut reply, ControlAction::ReplyResults),
        "play" => submit_search(app, tx, &body, &mut reply, ControlAction::PlayFirst),
        "queue_add" => submit_search(app, tx, &body, &mut reply, ControlAction::QueueFirst),
        "queue_list" => respond(&mut reply, control::ok(json!({
            "current": app.queue.current,
            "tracks": control::tracks_json(&app.queue.tracks),
        }))),
        "queue_remove" => respond(&mut reply, queue_remove(app, player, tx, &body)),
        "queue_play" => respond(&mut reply, queue_play(app, player, tx, &body)),
        "queue_clear" => respond(&mut reply, queue_clear(app, player, tx)),
        "control" => respond(&mut reply, palette(app, player, tx, &body)),
        "radio" => respond(&mut reply, radio(app, player, tx, &body)),
        "playlists_list" => respond(&mut reply, playlists_list(app)),
        "playlist_play" => respond(&mut reply, playlist_play(app, player, tx, &body)),
        "favorites_list" => respond(&mut reply, control::ok(control::tracks_json(&app.favorites.tracks))),
        other => respond(&mut reply, control::fail(format!("petición desconocida: «{other}»"))),
    }
}

/// Responde por el canal del request si aún está disponible.
fn respond(reply: &mut Option<tokio::sync::oneshot::Sender<Value>>, value: Value) {
    if let Some(sender) = reply.take() {
        let _ = sender.send(value);
    }
}

/// Estado de reproducción y cola en un solo objeto.
fn now_playing(app: &App) -> Value {
    let repeat = match app.repeat {
        super::state::RepeatMode::Off => "off",
        super::state::RepeatMode::All => "all",
        super::state::RepeatMode::One => "one",
    };
    control::ok(json!({
        "track": app.queue.current_track().map(control::track_json).unwrap_or(Value::Null),
        "position_secs": app.status.position.map(|d| d.as_secs()),
        "duration_secs": app.status.duration.map(|d| d.as_secs()),
        "paused": app.status.paused,
        "volume": app.status.volume,
        "queue_index": app.queue.current,
        "queue_len": app.queue.tracks.len(),
        "radio": app.radio,
        "radio_mode": app.radio_mode.name(),
        "repeat": repeat,
        "searching": app.searching,
    }))
}

/// Lanza la búsqueda y registra la respuesta pendiente. Devuelve None: el
/// reply ya está en `control_pending` (o ya se respondió con el error).
fn submit_search(
    app: &mut App,
    tx: &mpsc::UnboundedSender<AppMsg>,
    body: &Value,
    reply: &mut Option<tokio::sync::oneshot::Sender<Value>>,
    action: ControlAction,
) {
    if app.control_pending.is_some() {
        respond(reply, control::fail("hay otra petición de control en curso; inténtalo de nuevo"));
        return;
    }
    if app.searching {
        respond(reply, control::fail("hay una búsqueda en curso; inténtalo de nuevo"));
        return;
    }
    let Some(query) = body.get("query").and_then(Value::as_str).map(str::trim) else {
        respond(reply, control::fail("falta «query»"));
        return;
    };
    if query.is_empty() {
        respond(reply, control::fail("«query» está vacía"));
        return;
    }
    app.input = query.to_string();
    if action == ControlAction::PlayFirst {
        app.autoplay_first = true;
    }
    playback::submit_search(app, tx);
    let epoch = app.search_epoch;
    let sender = reply.take().expect("reply sin consumir en esta petición");
    app.control_pending = Some(ControlPending { epoch, reply: sender, action });
}

/// Elimina por índice con las mismas consecuencias que la tecla `d` en la
/// cola: si sonaba la actual, salta a la siguiente o detiene.
fn queue_remove(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    body: &Value,
) -> Value {
    let Some(index) = body.get("index").and_then(Value::as_u64).map(|i| i as usize) else {
        return control::fail("falta «index»");
    };
    if index >= app.queue.len() {
        return control::fail(format!("índice {index} fuera de la cola ({} pistas)", app.queue.len()));
    }
    match app.queue.remove(index) {
        Some(Removed::CurrentWithSuccessor(successor)) => {
            playback::play_queue_track(app, player, tx, successor);
            control::ok(json!({ "message": "eliminada; saltando a la siguiente" }))
        }
        Some(Removed::CurrentWithoutSuccessor) => {
            app.radio_epoch += 1;
            spawn_player(player, tx, |p| async move { p.stop().await });
            control::ok(json!({ "message": "eliminada la pista que sonaba; detenido" }))
        }
        Some(Removed::Other) => control::ok(json!({ "message": "eliminada de la cola" })),
        None => control::fail("no se pudo eliminar"),
    }
}

/// Reproduce la pista de la cola con el índice dado (0-based).
fn queue_play(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    body: &Value,
) -> Value {
    let Some(index) = body.get("index").and_then(Value::as_u64).map(|i| i as usize) else {
        return control::fail("falta «index»");
    };
    if index >= app.queue.len() {
        return control::fail(format!("índice {index} fuera de la cola ({} pistas)", app.queue.len()));
    }
    playback::play_queue_track(app, player, tx, index);
    let title = app
        .queue
        .current_track()
        .map(|t| t.title.clone())
        .unwrap_or_default();
    control::ok(json!({ "message": format!("reproduciendo «{title}» (índice {index})") }))
}

/// Vacía la cola y detiene la reproducción.
fn queue_clear(app: &mut App, player: &Arc<dyn Player>, tx: &mpsc::UnboundedSender<AppMsg>) -> Value {
    app.queue.tracks.clear();
    app.queue.current = None;
    app.queue_state.select(None);
    playback::stop_playback(app, player, tx);
    control::ok(json!({ "message": "cola vaciada" }))
}

/// Pasa un comando de la paleta (`:`) tal cual: `pause`, `next`, `vol 80`,
/// `theme dracula`, `radio on`… Misma tabla que la TUI.
fn palette(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    body: &Value,
) -> Value {
    let Some(command) = body.get("command").and_then(Value::as_str).map(str::trim) else {
        return control::fail("falta «command»");
    };
    if command.is_empty() {
        return control::fail("«command» está vacío");
    }
    if commands::run(app, player, tx, command) {
        control::ok(json!({ "message": format!("comando ejecutado: {command}") }))
    } else {
        control::fail(format!("comando desconocido: «{command}»"))
    }
}

/// Activa o desactiva el modo radio con los comandos de la paleta (misma
/// lógica, incluido el prefetch si la cola está por agotarse).
fn radio(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    body: &Value,
) -> Value {
    let Some(enabled) = body.get("enabled").and_then(Value::as_bool) else {
        return control::fail("falta «enabled» (true/false)");
    };
    let applied = if enabled { "radio on" } else { "radio off" };
    if !commands::run(app, player, tx, applied) {
        return control::fail("no se pudo cambiar el modo radio");
    }
    control::ok(json!({ "radio": app.radio, "mode": app.radio_mode.name() }))
}

/// Listado unificado: playlists inteligentes no vacías (flag `smart`) y
/// guardadas. Resolución de nombres en `playlist_play`: gana la guardada.
fn playlists_list(app: &App) -> Value {
    let mut items: Vec<Value> = crate::smart::SmartKind::ALL
        .into_iter()
        .filter_map(|kind| {
            let tracks = kind.tracks(&app.stats, &app.favorites);
            if tracks.is_empty() {
                None
            } else {
                Some(json!({ "name": kind.name(), "tracks": tracks.len(), "smart": true }))
            }
        })
        .collect();
    items.extend(
        app.playlists
            .items
            .iter()
            .map(|pl| json!({ "name": pl.name, "tracks": pl.tracks.len(), "smart": false })),
    );
    control::ok(Value::Array(items))
}

/// Sustituye la cola por una playlist y arranca su primera pista. Acepta
/// playlists guardadas y smart (RF-65): si el nombre coincide con una
/// guardada, gana esta (los datos del usuario mandan sobre la vista derivada).
fn playlist_play(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    body: &Value,
) -> Value {
    let Some(name) = body.get("name").and_then(Value::as_str).map(str::trim) else {
        return control::fail("falta «name»");
    };
    let stored = app
        .playlists
        .items
        .iter()
        .find(|pl| pl.name.eq_ignore_ascii_case(name))
        .map(|pl| (pl.name.clone(), pl.tracks.clone()));
    let (label, tracks) = match stored {
        Some((name, tracks)) => (format!("playlist «{name}»"), tracks),
        None => match crate::smart::SmartKind::from_name(name) {
            Some(kind) => {
                let tracks = kind.tracks(&app.stats, &app.favorites);
                (format!("playlist inteligente «{}»", kind.name()), tracks)
            }
            None => return control::fail(format!("playlist no encontrada: «{name}»")),
        },
    };
    if tracks.is_empty() {
        return control::fail(format!("la {label} está vacía"));
    }
    let len = tracks.len();
    app.queue.tracks = tracks;
    app.queue.current = None;
    playback::play_queue_track(app, player, tx, 0);
    control::ok(json!({ "message": format!("sonando {label} ({len} pistas)") }))
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::config::Config;
    use crate::providers::{ProviderKind, Track};
    use anyhow::Result;

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

    fn test_app() -> App {
        App::with_data_dir(
            &Config::default(),
            &std::env::temp_dir().join(format!(
                "shellmusic-control-test-{}-{:016x}",
                std::process::id(),
                crate::state_file::random_suffix()
            )),
        )
        .unwrap()
    }

    #[test]
    fn playlists_list_flags_smart_and_skips_empty() {
        let mut app = test_app();
        let pl = app.playlists.create("mix").unwrap();
        app.playlists.add_track(pl, track("a"));

        // Sin estadísticas no hay smart no vacías: solo la guardada.
        let data = playlists_list(&app);
        let items = data["data"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["name"], "mix");
        assert_eq!(items[0]["smart"], false);

        // Con una reproducción aparece la smart con su flag y conteo.
        app.stats.record(&track("a"));
        let items = playlists_list(&app)["data"].as_array().unwrap().clone();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["name"], "Más escuchadas");
        assert_eq!(items[0]["tracks"], 1);
        assert_eq!(items[0]["smart"], true);
        assert_eq!(items[1]["name"], "mix");
        assert_eq!(items[1]["smart"], false);
    }

    #[tokio::test]
    async fn playlist_play_resolves_smart_by_name() {
        let mut app = test_app();
        app.stats.record(&track("a"));
        app.stats.record(&track("b"));
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();

        let response = playlist_play(
            &mut app,
            &player,
            &tx,
            &json!({ "name": "más escuchadas" }),
        );
        assert_eq!(response["ok"], true);
        assert_eq!(app.queue.tracks.len(), 2, "la cola queda sustituida");
        assert_eq!(app.queue.current, Some(0), "arranca la primera pista");
        assert!(
            response["data"]["message"]
                .as_str()
                .unwrap()
                .contains("playlist inteligente"),
            "mensaje: {response}"
        );
    }

    #[tokio::test]
    async fn playlist_play_stored_wins_over_smart_name() {
        // Una playlist guardada con el nombre de una smart tiene prioridad:
        // los datos del usuario mandan sobre la vista derivada.
        let mut app = test_app();
        let pl = app.playlists.create("Más escuchadas").unwrap();
        app.playlists.add_track(pl, track("guardada"));
        app.stats.record(&track("stat"));
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();

        let response = playlist_play(&mut app, &player, &tx, &json!({ "name": "Más Escuchadas" }));
        assert_eq!(response["ok"], true);
        assert_eq!(app.queue.tracks.len(), 1);
        assert_eq!(app.queue.tracks[0].id, "guardada");
        assert!(
            !response["data"]["message"]
                .as_str()
                .unwrap()
                .contains("inteligente"),
            "mensaje: {response}"
        );
    }

    #[tokio::test]
    async fn playlist_play_unknown_and_empty_fail() {
        let mut app = test_app();
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();

        let response = playlist_play(&mut app, &player, &tx, &json!({ "name": "inexistente" }));
        assert_eq!(response["ok"], false);

        // Smart conocida pero vacía (sin datos de escucha): error accionable.
        let response = playlist_play(&mut app, &player, &tx, &json!({ "name": "redescubrir" }));
        assert_eq!(response["ok"], false);
        assert!(
            response["error"].as_str().unwrap().contains("vacía"),
            "error: {response}"
        );

        let response = playlist_play(&mut app, &player, &tx, &json!({}));
        assert_eq!(response["ok"], false);
        assert!(response["error"].as_str().unwrap().contains("name"));
    }
}
