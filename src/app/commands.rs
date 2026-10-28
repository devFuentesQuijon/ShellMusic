//! Paleta de comandos (`:`): acciones globales ejecutables escribiendo su
//! nombre, para descubrir atajos sin memorizarlos. La tabla es estática;
//! el filtrado en vivo muestra los comandos cuya etiqueta contiene lo
//! escrito y Enter ejecuta el de prefijo más largo que coincida.

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::player::Player;
use crate::queue::Queue;

use super::AppMsg;
use super::playback::{change_volume, play_adjacent, spawn_player};
use super::state::{App, Message, Overlay};

type CommandFn = fn(&mut App, &Arc<dyn Player>, &mpsc::UnboundedSender<AppMsg>, &str);

struct Command {
    /// Etiqueta mostrada en la paleta.
    label: &'static str,
    /// Prefijo de despacho: coincide si la entrada es igual o empieza por
    /// `name` + espacio; el resto llega como argumento.
    name: &'static str,
    run: CommandFn,
}

const COMMANDS: &[Command] = &[
    Command { label: "pause", name: "pause", run: pause },
    Command { label: "stop", name: "stop", run: stop },
    Command { label: "next", name: "next", run: next },
    Command { label: "prev", name: "prev", run: prev },
    Command { label: "vol up", name: "vol up", run: vol_up },
    Command { label: "vol down", name: "vol down", run: vol_down },
    Command { label: "vol <n>", name: "vol", run: vol_set },
    Command { label: "theme <nombre>", name: "theme", run: theme_set },
    Command { label: "radio on", name: "radio on", run: radio_on },
    Command { label: "radio off", name: "radio off", run: radio_off },
    Command { label: "stats", name: "stats", run: stats },
    Command { label: "playlists", name: "playlists", run: playlists },
    Command { label: "help", name: "help", run: help },
    Command { label: "quit", name: "quit", run: quit },
];

/// Comandos visibles para una entrada: con entrada vacía, todos; si no, los
/// cuya etiqueta contiene la primera palabra escrita (o la entrada entera).
pub(crate) fn matching(input: &str) -> Vec<&'static str> {
    let input = input.trim();
    let first = input.split_whitespace().next().unwrap_or("");
    COMMANDS
        .iter()
        .filter(|c| {
            input.is_empty() || c.label.contains(first) || c.label.contains(input)
        })
        .map(|c| c.label)
        .take(8)
        .collect()
}

/// Despacha la entrada contra la tabla: prefijo más largo que coincida.
/// Devuelve false si no hay comando para esa entrada.
pub(crate) fn run(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    input: &str,
) -> bool {
    let input = input.trim();
    let Some(command) = COMMANDS
        .iter()
        .filter(|c| input == c.name || input.starts_with(&format!("{} ", c.name)))
        .max_by_key(|c| c.name.len())
    else {
        return false;
    };
    let arg = input[command.name.len()..].trim();
    (command.run)(app, player, tx, arg);
    true
}

/// Nombres de los comandos (para el test de unicidad).
#[cfg(test)]
fn names() -> Vec<&'static str> {
    COMMANDS.iter().map(|c| c.name).collect()
}

fn pause(_: &mut App, player: &Arc<dyn Player>, tx: &mpsc::UnboundedSender<AppMsg>, _: &str) {
    spawn_player(player, tx, |p| async move { p.toggle_pause().await });
}

fn stop(app: &mut App, player: &Arc<dyn Player>, tx: &mpsc::UnboundedSender<AppMsg>, _: &str) {
    app.queue.current = None;
    app.radio_epoch += 1;
    app.playback_active = false;
    app.mpv_appended = None;
    app.message = Some(Message::info("detenido"));
    spawn_player(player, tx, |p| async move { p.stop().await });
}

fn next(app: &mut App, player: &Arc<dyn Player>, tx: &mpsc::UnboundedSender<AppMsg>, _: &str) {
    play_adjacent(app, player, tx, Queue::next_index, "fin de la cola");
}

fn prev(app: &mut App, player: &Arc<dyn Player>, tx: &mpsc::UnboundedSender<AppMsg>, _: &str) {
    play_adjacent(app, player, tx, Queue::prev_index, "inicio de la cola");
}

fn vol_up(app: &mut App, player: &Arc<dyn Player>, tx: &mpsc::UnboundedSender<AppMsg>, _: &str) {
    change_volume(app, player, tx, 5);
}

fn vol_down(app: &mut App, player: &Arc<dyn Player>, tx: &mpsc::UnboundedSender<AppMsg>, _: &str) {
    change_volume(app, player, tx, -5);
}

/// `vol <n>`: fija el volumen absoluto (0-100).
fn vol_set(app: &mut App, player: &Arc<dyn Player>, tx: &mpsc::UnboundedSender<AppMsg>, arg: &str) {
    let Ok(target) = arg.parse::<i16>() else {
        app.message = Some(Message::error(format!("uso: vol <0-100> (recibido «{arg}»)")));
        return;
    };
    let delta = target.clamp(0, 100) - i16::from(app.status.volume);
    change_volume(app, player, tx, delta);
}

/// `theme <nombre>`: aplica un preset empaquetado en caliente.
fn theme_set(app: &mut App, _: &Arc<dyn Player>, _: &mpsc::UnboundedSender<AppMsg>, arg: &str) {
    match crate::theme::find_preset(arg) {
        Some(preset) => {
            app.theme = crate::theme::Theme::resolve(&preset).0;
            app.message = Some(Message::info(format!("tema «{arg}» aplicado")));
        }
        None => {
            app.message = Some(Message::error(format!(
                "tema desconocido: «{arg}». Disponibles: {}",
                crate::theme::preset_names().join(", ")
            )));
        }
    }
}

fn radio_on(app: &mut App, _: &Arc<dyn Player>, tx: &mpsc::UnboundedSender<AppMsg>, _: &str) {
    if !app.radio {
        app.toggle_radio();
        // Como la tecla r: si suena la última pista de la cola, prefetch ya.
        super::playback::radio_prefetch(app, tx);
    } else {
        app.message = Some(Message::info("el modo radio ya está activado"));
    }
}

fn radio_off(app: &mut App, _: &Arc<dyn Player>, _: &mpsc::UnboundedSender<AppMsg>, _: &str) {
    if app.radio {
        app.toggle_radio();
    } else {
        app.message = Some(Message::info("el modo radio ya está desactivado"));
    }
}

fn stats(app: &mut App, _: &Arc<dyn Player>, _: &mpsc::UnboundedSender<AppMsg>, _: &str) {
    app.open_stats();
}

fn playlists(app: &mut App, _: &Arc<dyn Player>, _: &mpsc::UnboundedSender<AppMsg>, _: &str) {
    app.overlay = Overlay::List;
    app.pending_delete = None;
    if app.overlay_list_state.selected().is_none() {
        app.overlay_list_select(0);
    }
}

fn help(app: &mut App, _: &Arc<dyn Player>, _: &mpsc::UnboundedSender<AppMsg>, _: &str) {
    app.overlay = Overlay::Help;
}

fn quit(app: &mut App, _: &Arc<dyn Player>, _: &mpsc::UnboundedSender<AppMsg>, _: &str) {
    app.should_quit = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_names_are_unique() {
        let names = names();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names.len(), sorted.len(), "nombres de comando duplicados");
    }

    #[test]
    fn matching_filters_by_first_word() {
        // Entrada vacía: muestra hasta 8 (límite de la paleta).
        let all = matching("");
        assert_eq!(all.len(), 8);
        // Primera palabra: «vol» muestra las tres variantes.
        let vols = matching("vol");
        assert_eq!(vols, vec!["vol up", "vol down", "vol <n>"]);
        // Entrada con argumento: sigue mostrando el comando base.
        assert!(matching("vol 5").contains(&"vol <n>"));
        // Sin coincidencias.
        assert!(matching("zzzz").is_empty());
    }

    #[test]
    fn run_dispatches_longest_prefix_with_arg() {
        // El arg es todo lo que sobra tras el prefijo más largo.
        assert_eq!(arg_for("vol 42"), Some(("vol", "42".to_string())));
        assert_eq!(arg_for("vol up"), Some(("vol up", String::new())));
        assert_eq!(arg_for("theme dracula"), Some(("theme", "dracula".to_string())));
        assert_eq!(arg_for("inventado"), None);
    }

    /// Versión pura del despacho: (nombre del comando, arg) sin ejecutar.
    fn arg_for(input: &str) -> Option<(&'static str, String)> {
        let input = input.trim();
        COMMANDS
            .iter()
            .filter(|c| input == c.name || input.starts_with(&format!("{} ", c.name)))
            .max_by_key(|c| c.name.len())
            .map(|c| (c.name, input[c.name.len()..].trim().to_string()))
    }
}
