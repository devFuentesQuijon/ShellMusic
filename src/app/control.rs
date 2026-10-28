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
        "playlists_list" => respond(&mut reply, control::ok(json!(
            app.playlists
                .items
                .iter()
                .map(|pl| json!({ "name": pl.name, "tracks": pl.tracks.len() }))
                .collect::<Vec<_>>()
        ))),
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

/// Sustituye la cola por una playlist y arranca su primera pista.
fn playlist_play(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    body: &Value,
) -> Value {
    let Some(name) = body.get("name").and_then(Value::as_str).map(str::trim) else {
        return control::fail("falta «name»");
    };
    let Some(pos) = app
        .playlists
        .items
        .iter()
        .position(|pl| pl.name.eq_ignore_ascii_case(name))
    else {
        return control::fail(format!("playlist no encontrada: «{name}»"));
    };
    let (name, len) = {
        let pl = &app.playlists.items[pos];
        if pl.tracks.is_empty() {
            return control::fail(format!("la playlist «{}» está vacía", pl.name));
        }
        app.queue.tracks = pl.tracks.clone();
        (pl.name.clone(), pl.tracks.len())
    };
    app.queue.current = None;
    playback::play_queue_track(app, player, tx, 0);
    control::ok(json!({ "message": format!("sonando playlist «{name}» ({len} pistas)") }))
}
