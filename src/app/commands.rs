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
use super::state::{App, Focus, Message, Overlay, SleepTimer};
use crate::providers::Track;

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
    Command {
        label: "pause",
        name: "pause",
        run: pause,
    },
    Command {
        label: "stop",
        name: "stop",
        run: stop,
    },
    Command {
        label: "next",
        name: "next",
        run: next,
    },
    Command {
        label: "prev",
        name: "prev",
        run: prev,
    },
    Command {
        label: "vol up",
        name: "vol up",
        run: vol_up,
    },
    Command {
        label: "vol down",
        name: "vol down",
        run: vol_down,
    },
    Command {
        label: "vol <n>",
        name: "vol",
        run: vol_set,
    },
    Command {
        label: "theme <nombre>",
        name: "theme",
        run: theme_set,
    },
    Command {
        label: "sleep <min|off|track>",
        name: "sleep",
        run: sleep_set,
    },
    Command {
        label: "sort title",
        name: "sort title",
        run: sort_title,
    },
    Command {
        label: "sort artist",
        name: "sort artist",
        run: sort_artist,
    },
    Command {
        label: "radio on",
        name: "radio on",
        run: radio_on,
    },
    Command {
        label: "radio off",
        name: "radio off",
        run: radio_off,
    },
    Command {
        label: "ytm likes",
        name: "ytm likes",
        run: ytm_likes,
    },
    Command {
        label: "ytm import <nombre>",
        name: "ytm import",
        run: ytm_import,
    },
    Command {
        label: "ytm home",
        name: "ytm home",
        run: ytm_home,
    },
    Command {
        label: "ytm radio",
        name: "ytm radio",
        run: ytm_radio,
    },
    Command {
        label: "stats",
        name: "stats",
        run: stats,
    },
    Command {
        label: "playlists",
        name: "playlists",
        run: playlists,
    },
    Command {
        label: "help",
        name: "help",
        run: help,
    },
    Command {
        label: "quit",
        name: "quit",
        run: quit,
    },
];

/// Comandos visibles para una entrada: con entrada vacía, todos; si no, los
/// cuya etiqueta contiene la primera palabra escrita (o la entrada entera).
pub(crate) fn matching(input: &str) -> Vec<&'static str> {
    let input = input.trim();
    let first = input.split_whitespace().next().unwrap_or("");
    COMMANDS
        .iter()
        .filter(|c| input.is_empty() || c.label.contains(first) || c.label.contains(input))
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
    super::playback::stop_playback(app, player, tx);
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
        app.message = Some(Message::error(format!(
            "uso: vol <0-100> (recibido «{arg}»)"
        )));
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

/// `sleep <arg>`: programa la parada. "off"/"0" lo cancela; "track" detiene
/// al terminar la pista en curso; un número (sufijo `s` = segundos, `m` o
/// nada = minutos) detiene al cabo de ese tiempo.
fn sleep_set(app: &mut App, _: &Arc<dyn Player>, _: &mpsc::UnboundedSender<AppMsg>, arg: &str) {
    match parse_sleep(arg) {
        Ok(Some(minutes)) => {
            let duration = std::time::Duration::from_secs(minutes * 60);
            app.sleep_timer = Some(SleepTimer::At(std::time::Instant::now() + duration));
            app.message = Some(Message::info(format!("sleep: stop en {minutes} min")));
        }
        Ok(None) => {
            app.sleep_timer = Some(SleepTimer::AfterTrack);
            app.message = Some(Message::info("sleep: stop al terminar la pista"));
        }
        Err(()) if arg.trim().eq_ignore_ascii_case("off") || arg.trim() == "0" => {
            if app.sleep_timer.take().is_some() {
                app.message = Some(Message::info("sleep timer cancelado"));
            } else {
                app.message = Some(Message::info("no hay sleep timer activo"));
            }
        }
        Err(()) => {
            app.message = Some(Message::error(format!(
                "uso: sleep <minutos|off|track> (recibido «{arg}»)"
            )));
        }
    }
}

/// Parsea el argumento de `sleep`: Ok(Some(minutos)), Ok(None) = «track»,
/// Err(()) = argumento inválido (el caller distingue «off»).
fn parse_sleep(arg: &str) -> Result<Option<u64>, ()> {
    let arg = arg.trim().to_lowercase();
    if matches!(arg.as_str(), "track" | "pista") {
        return Ok(None);
    }
    let (digits, unit) = arg
        .strip_suffix(['s', 'm'])
        .map_or((arg.as_str(), 'm'), |d| {
            (d, arg.chars().last().unwrap_or('m'))
        });
    let value: u64 = digits.trim().parse().map_err(|_| ())?;
    if value == 0 {
        return Err(());
    }
    match unit {
        's' => Ok(Some(value.div_ceil(60))),
        _ => Ok(Some(value)),
    }
}

/// `sort title` / `sort artist`: ordenan el panel con el foco (Cola o
/// Favoritos); en contenido de playlist se ordena con `s`/`S` dentro del overlay.
fn sort_title(app: &mut App, _: &Arc<dyn Player>, _: &mpsc::UnboundedSender<AppMsg>, _: &str) {
    sort_focused(app, crate::providers::cmp_by_title, "título");
}

fn sort_artist(app: &mut App, _: &Arc<dyn Player>, _: &mpsc::UnboundedSender<AppMsg>, _: &str) {
    sort_focused(app, crate::providers::cmp_by_artist, "artista");
}

fn sort_focused(
    app: &mut App,
    cmp: fn(&Track, &Track) -> std::cmp::Ordering,
    what: &str,
) {
    match app.focus {
        Focus::Favorites => {
            if app.favorites.tracks.len() < 2 {
                app.message = Some(Message::info("favoritos demasiado cortos para ordenar"));
                return;
            }
            app.favorites.tracks.sort_by(cmp);
            app.save_favorites();
            app.favorites_select(0);
            app.message = Some(Message::info(format!("favoritos ordenados por {what}")));
        }
        Focus::Queue => {
            if app.queue.len() < 2 {
                app.message = Some(Message::info("cola demasiado corta para ordenar"));
                return;
            }
            app.queue.sort_by(cmp);
            // La selección vuelve a la pista actual (reubicada por el orden).
            let current = app.queue.current.unwrap_or(0);
            app.queue_select_real(current);
            app.message = Some(Message::info(format!("cola ordenada por {what}")));
        }
        Focus::Browsing | Focus::Searching => {
            app.message = Some(Message::info("nada que ordenar aquí: foco en Cola o Favoritos"));
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

/// Comprueba que YouTube Music esté usable y lanza el comando del bridge en
/// segundo plano; el resultado llega por `AppMsg::YtmDone`. Devuelve false
/// (con mensaje accionable) cuando no se puede.
fn ytm_spawn(
    app: &mut App,
    tx: &mpsc::UnboundedSender<AppMsg>,
    op: super::YtmOp,
    args: &[&str],
    pending: &str,
) -> bool {
    if !app.ytm {
        app.message = Some(Message::info(
            "YouTube Music no está activado: alta con «smusic --ytm-auth» y [ytm] enabled = true en config.toml",
        ));
        return false;
    }
    if !crate::ytm::ready(&app.data_dir) {
        app.message = Some(Message::info(
            "faltan los headers de YouTube Music: ejecuta «smusic --ytm-auth»",
        ));
        return false;
    }
    if app.ytm_busy {
        app.message = Some(Message::info(
            "hay otra operación de YouTube Music en curso; espera a que termine",
        ));
        return false;
    }
    app.ytm_busy = true;
    app.message = Some(Message::info(pending));
    let data_dir = app.data_dir.clone();
    let tx = tx.clone();
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    tokio::spawn(async move {
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
        let result = crate::ytm::run(&data_dir, &refs)
            .await
            .map_err(|err| format!("{err:#}"));
        let _ = tx.send(AppMsg::YtmDone(op, result));
    });
    true
}

/// `ytm likes`: importa los likes de la cuenta como favoritos locales.
fn ytm_likes(app: &mut App, _: &Arc<dyn Player>, tx: &mpsc::UnboundedSender<AppMsg>, _: &str) {
    ytm_spawn(
        app,
        tx,
        super::YtmOp::Likes,
        &["likes"],
        "YouTube Music: descargando likes…",
    );
}

/// `ytm import <nombre>`: importa la playlist de la biblioteca cuyo título
/// contiene <nombre> como playlist local.
fn ytm_import(app: &mut App, _: &Arc<dyn Player>, tx: &mpsc::UnboundedSender<AppMsg>, arg: &str) {
    let name = arg.trim();
    if name.is_empty() {
        app.message = Some(Message::info("uso: ytm import <nombre de la playlist>"));
        return;
    }
    ytm_spawn(
        app,
        tx,
        super::YtmOp::Import(name.to_string()),
        &["import", name],
        "YouTube Music: importando playlist…",
    );
}

/// `ytm home`: carga el feed Home (recomendaciones de la cuenta) en Resultados.
fn ytm_home(app: &mut App, _: &Arc<dyn Player>, tx: &mpsc::UnboundedSender<AppMsg>, _: &str) {
    ytm_spawn(
        app,
        tx,
        super::YtmOp::Home,
        &["home"],
        "YouTube Music: descargando Home…",
    );
}

/// `ytm radio`: radio personalizada (watch playlist) de la pista que suena,
/// en Resultados. Es la semilla del modo radio de la propia cuenta.
fn ytm_radio(app: &mut App, _: &Arc<dyn Player>, tx: &mpsc::UnboundedSender<AppMsg>, _: &str) {
    let Some(track) = app.queue.current_track().cloned() else {
        app.message = Some(Message::info("nada sonando: la radio personalizada parte de la pista actual"));
        return;
    };
    if track.provider != crate::providers::ProviderKind::Youtube {
        app.message = Some(Message::info("la radio personalizada solo aplica a pistas de YouTube"));
        return;
    }
    ytm_spawn(
        app,
        tx,
        super::YtmOp::Radio,
        &["radio", &track.id],
        "YouTube Music: creando radio personalizada…",
    );
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
        assert_eq!(
            arg_for("theme dracula"),
            Some(("theme", "dracula".to_string()))
        );
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

    #[test]
    fn parse_sleep_minutes_seconds_track_and_invalid() {
        // Minutos (con o sin sufijo m) y redondeo de segundos a minuto arriba.
        assert_eq!(parse_sleep("15"), Ok(Some(15)));
        assert_eq!(parse_sleep("15m"), Ok(Some(15)));
        assert_eq!(parse_sleep("90s"), Ok(Some(2)), "90 s redondea a 2 min");
        assert_eq!(parse_sleep("30s"), Ok(Some(1)));
        // «Al terminar la pista».
        assert_eq!(parse_sleep("track"), Ok(None));
        assert_eq!(parse_sleep("pista"), Ok(None));
        // Inválidos: el caller distingue «off»/«0» del resto de basura.
        assert_eq!(parse_sleep("0"), Err(()));
        assert_eq!(parse_sleep("off"), Err(()));
        assert_eq!(parse_sleep("xyz"), Err(()));
        assert_eq!(parse_sleep("5h"), Err(()));
        assert_eq!(parse_sleep(""), Err(()));
    }
}
