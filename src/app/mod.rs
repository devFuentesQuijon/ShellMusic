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
use crate::system::{MediaCommand, SystemIntegration};
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
    LyricsDone(Result<String, String>),
    CoverDone(Result<CoverImage, String>),
}

/// Carátula descargada y decodificada, lista para convertir en protocolo.
pub struct CoverImage {
    pub track: (ProviderKind, String),
    pub image: image::DynamicImage,
}

pub async fn run(
    terminal: &mut DefaultTerminal,
    providers: Vec<(ProviderKind, Arc<dyn MusicProvider>)>,
    player: Arc<dyn Player>,
    mut player_events: mpsc::UnboundedReceiver<PlayerEvent>,
    config: Config,
    picker: ratatui_image::picker::Picker,
) -> Result<()> {
    let mut app = App::new(&config)?;
    app.picker = Some(picker);
    if !providers.is_empty() {
        app.providers = providers;
        app.active_provider = 0;
    }
    let (tx, mut rx) = mpsc::unbounded_channel::<AppMsg>();
    let mut keys = EventStream::new();

    // Integración con el SO: MPRIS/media controls (Linux) y notificaciones.
    let (media_tx, mut media_rx) = mpsc::unbounded_channel::<MediaCommand>();
    let mut system = SystemIntegration::new(&config.integrations, media_tx);
    if let Some(warning) = system.startup_warning.take()
        && app.message.is_none()
    {
        app.message = Some(Message::info(warning));
    }

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
            Some(command) = media_rx.recv() => {
                handle_media_command(&mut app, command, &player, &tx);
            }
            Some(event) = player_events.recv() => match event {
                PlayerEvent::Notice(PlayerNotice { text, is_error }) => {
                    app.message = Some(Message { text, is_error });
                }
                PlayerEvent::TrackEnded => autoplay_next(&mut app, &player, &tx),
            },
        }
        set_tab_title(&desired_title(&app), &mut last_title);
        system.update(&app);
        if app.should_quit {
            break;
        }
    }
    app.save_session(app.status.position);
    set_tab_title("", &mut last_title);
    Ok(())
}

/// Ejecuta un comando de los media controls del SO (MPRIS en Linux).
fn handle_media_command(
    app: &mut App,
    command: MediaCommand,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
    use playback::{change_volume, play_adjacent, seek_by, spawn_player};
    use crate::queue::Queue;
    match command {
        MediaCommand::Play | MediaCommand::Pause | MediaCommand::Toggle => {
            spawn_player(player, tx, |p| async move { p.toggle_pause().await });
        }
        MediaCommand::Stop => {
            app.queue.current = None;
            app.radio_epoch += 1;
            app.playback_active = false;
            app.message = Some(Message::info("detenido"));
            spawn_player(player, tx, |p| async move { p.stop().await });
        }
        MediaCommand::Next => {
            play_adjacent(app, player, tx, Queue::next_index, "fin de la cola");
        }
        MediaCommand::Previous => {
            play_adjacent(app, player, tx, Queue::prev_index, "inicio de la cola");
        }
        MediaCommand::SeekForward(delta) => {
            seek_by(app, player, tx, delta.as_secs().min(i32::MAX as u64) as i32);
        }
        MediaCommand::SeekBackward(delta) => {
            seek_by(app, player, tx, -(delta.as_secs().min(i32::MAX as u64) as i32));
        }
        MediaCommand::SetPosition(target) => {
            // Seek relativo respecto a la posición actual conocida.
            if let Some(current) = app.status.position {
                let delta = target.as_secs_f64() - current.as_secs_f64();
                seek_by(app, player, tx, delta as i32);
            }
        }
        MediaCommand::SetVolume(volume) => {
            let current = i16::from(app.status.volume);
            let target = (volume.clamp(0.0, 1.0) * 100.0).round() as i16;
            change_volume(app, player, tx, target - current);
        }
    }
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
    use crate::providers::ProviderKind;

    #[test]
    fn sanitize_title_drops_control_chars() {
        assert_eq!(sanitize_title("hola\u{1b}]mundo\n"), "hola]mundo");
        assert_eq!(sanitize_title("ShellMusic"), "ShellMusic");
    }

    struct MockPlayer;

    #[async_trait::async_trait]
    impl Player for MockPlayer {
        async fn play(&self, _url: &str, _start: Option<Duration>) -> Result<()> {
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

    fn app_with_two_tracks() -> App {
        let mut app = App::with_data_dir(
            &Config::default(),
            &std::env::temp_dir().join(format!(
                "shellmusic-media-test-{}-{:016x}",
                std::process::id(),
                crate::state_file::random_suffix()
            )),
        )
        .unwrap();
        for id in ["a", "b"] {
            app.queue.add(Track {
                id: id.to_string(),
                title: format!("título {id}"),
                artist: "artista".to_string(),
                duration: None,
                source_url: format!("https://example.com/{id}"),
                provider: ProviderKind::Youtube,
            });
        }
        app.queue.current = Some(0);
        app
    }

    #[tokio::test]
    async fn media_next_and_previous_move_the_queue() {
        let mut app = app_with_two_tracks();
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();

        handle_media_command(&mut app, MediaCommand::Next, &player, &tx);
        assert_eq!(app.queue.current, Some(1));
        handle_media_command(&mut app, MediaCommand::Previous, &player, &tx);
        assert_eq!(app.queue.current, Some(0));
    }

    #[tokio::test]
    async fn media_stop_clears_current_track() {
        let mut app = app_with_two_tracks();
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();

        handle_media_command(&mut app, MediaCommand::Stop, &player, &tx);
        assert_eq!(app.queue.current, None);
        assert!(!app.playback_active);
    }

    #[tokio::test]
    async fn media_set_volume_maps_fraction_to_percent() {
        let mut app = app_with_two_tracks();
        app.status.volume = 50;
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();

        handle_media_command(&mut app, MediaCommand::SetVolume(0.7), &player, &tx);
        assert_eq!(app.status.volume, 70);
        handle_media_command(&mut app, MediaCommand::SetVolume(2.0), &player, &tx);
        assert_eq!(app.status.volume, 100, "se clama al 100");
    }

    #[tokio::test]
    async fn media_set_position_seeks_relative_to_current() {
        let mut app = app_with_two_tracks();
        app.status.position = Some(Duration::from_secs(50));
        let player: Arc<dyn Player> = Arc::new(MockPlayer);
        let (tx, _rx) = mpsc::unbounded_channel();

        handle_media_command(
            &mut app,
            MediaCommand::SetPosition(Duration::from_secs(80)),
            &player,
            &tx,
        );
        assert!(
            app.message
                .as_ref()
                .is_some_and(|m| m.text.contains("+30")),
            "seek +30 esperado"
        );
    }
}
