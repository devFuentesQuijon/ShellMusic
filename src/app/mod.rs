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

pub use state::{App, Focus, Message, NameContext, Overlay, RadioMode, RepeatMode};

use input::handle_key;
use playback::{autoplay_next, handle_msg};

const TICK: Duration = Duration::from_millis(500);

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
        if app.should_quit {
            break;
        }
    }
    Ok(())
}
