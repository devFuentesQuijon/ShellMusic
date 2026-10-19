use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyEventKind};
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

pub use state::{App, Focus, Message, NameContext, Overlay};

use input::handle_key;
use playback::{autoplay_next, handle_msg};

const TICK: Duration = Duration::from_millis(500);

pub(super) enum AppMsg {
    SearchDone(Result<Vec<Track>, String>),
    Error(String),
    PlaybackStarted,
}

pub async fn run(
    terminal: &mut DefaultTerminal,
    providers: Vec<(ProviderKind, Arc<dyn MusicProvider>)>,
    player: Arc<dyn Player>,
    mut player_events: mpsc::UnboundedReceiver<PlayerEvent>,
    config: Config,
) -> Result<()> {
    let mut app = App::new(&config);
    if !providers.is_empty() {
        app.providers = providers;
        app.active_provider = 0;
    }
    let (tx, mut rx) = mpsc::unbounded_channel::<AppMsg>();
    let mut keys = EventStream::new();
    let fps = u64::from(app.visualizer_fps());

    loop {
        terminal.draw(|frame| tui::draw(frame, &mut app))?;
        // Con el visualizador activo el tick sube a su fps; si no, 500 ms.
        let tick_period = if app.visualizer.is_some() {
            Duration::from_millis(1000 / fps)
        } else {
            TICK
        };
        let tick = tokio::time::sleep(tick_period);
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
            Some(msg) = rx.recv() => handle_msg(&mut app, msg),
            Some(event) = player_events.recv() => match event {
                PlayerEvent::Notice(PlayerNotice { text, is_error }) => {
                    app.message = Some(Message { text, is_error });
                }
                PlayerEvent::TrackEnded => autoplay_next(&mut app, &player, &tx),
            },
        }
        if app.should_quit {
            break;
        }
    }
    Ok(())
}
