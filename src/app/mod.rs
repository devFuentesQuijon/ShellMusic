use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyEventKind};
use crossterm::terminal::SetTitle;
use futures_util::StreamExt;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::player::{Player, PlayerEvent, PlayerNotice};
use crate::providers::{MusicProvider, ProviderKind, Track};
use crate::tui;

mod input;
mod playback;
mod state;

pub use state::{App, Focus, Message, NameContext, Overlay, PathAction, RadioMode, RepeatMode};

use input::handle_key;
use playback::{autoplay_next, handle_msg, play_queue_track_from};

const TICK: Duration = Duration::from_millis(500);
const APP_TITLE: &str = "ShellMusic";

pub(super) enum AppMsg {
    SearchDone(Result<Vec<Track>, String>),
    Error(String),
    PlaybackStarted,
    RadioDone(String, Result<Vec<Track>, String>, u64),
}

pub async fn run(
    terminal: &mut DefaultTerminal,
    providers: Vec<(ProviderKind, Arc<dyn MusicProvider>)>,
    player: Arc<dyn Player>,
    mut player_events: mpsc::UnboundedReceiver<PlayerEvent>,
    config: Config,
) -> Result<()> {
    let mut app = App::new(&config)?;
    if !providers.is_empty() {
        app.providers = providers;
        app.active_provider = 0;
    }
    let (tx, mut rx) = mpsc::unbounded_channel::<AppMsg>();
    let mut keys = EventStream::new();

    // Título de la pestaña: «ShellMusic» en reposo, pista en reproducción.
    let mut last_title = String::new();
    set_tab_title(APP_TITLE, &mut last_title);

    // Cola persistente: reanuda la pista que sonaba al cerrar la sesión
    // anterior, desde la posición guardada.
    if let Some(current) = app.queue.current {
        let start = app.pending_position.take();
        if app.message.is_none() {
            app.message = Some(Message::info("sesión restaurada"));
        }
        play_queue_track_from(&mut app, &player, &tx, current, start);
    }

    loop {
        terminal.draw(|frame| tui::draw(frame, &mut app))?;
        let tick = tokio::time::sleep(TICK);
        tokio::select! {
            event = keys.next() => {
                if let Some(Ok(Event::Key(key))) = event
                    && key.kind == KeyEventKind::Press
                {
                    handle_key(&mut app, key, &player, &tx);
                }
            }
            _ = tick => {
                if let Ok(status) = player.status().await {
                    app.status = status;
                }
            }
            Some(msg) = rx.recv() => handle_msg(&mut app, msg, &player, &tx),
            Some(event) = player_events.recv() => match event {
                PlayerEvent::Notice(PlayerNotice { text, is_error }) => {
                    app.message = Some(Message { text, is_error });
                }
                PlayerEvent::TrackEnded => autoplay_next(&mut app, &player, &tx),
            },
        }
        set_tab_title(&desired_title(&app), &mut last_title);
        if app.should_quit {
            break;
        }
    }
    app.save_session(app.status.position);
    set_tab_title("", &mut last_title);
    Ok(())
}

/// Título deseado de la pestaña: pista en reproducción o nombre de la app.
fn desired_title(app: &App) -> String {
    match app.queue.current_track() {
        Some(track) => sanitize_title(&format!(
            "♪ {} — {} · {APP_TITLE}",
            track.artist, track.title
        )),
        None => APP_TITLE.to_string(),
    }
}

/// Las secuencias de control romperían la OSC 2; fuera.
fn sanitize_title(title: &str) -> String {
    title.chars().filter(|c| !c.is_control()).collect()
}

/// Actualiza el título de la pestaña solo cuando cambia (OSC 2).
fn set_tab_title(title: &str, last: &mut String) {
    if title == last {
        return;
    }
    if crossterm::execute!(std::io::stdout(), SetTitle(title)).is_ok() {
        *last = title.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_title_drops_control_chars() {
        assert_eq!(sanitize_title("hola\u{1b}]mundo\n"), "hola]mundo");
        assert_eq!(sanitize_title("ShellMusic"), "ShellMusic");
    }
}
