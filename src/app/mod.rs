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

// Reproductor intercambiable: si mpv muere en marcha se respawnea con
// el volumen vigente. Guarda anti-bucle: máx. 3 respawns en 60 s.
type RespawnResult = Result<(Arc<dyn Player>, mpsc::UnboundedReceiver<PlayerEvent>), String>;
type RespawnRx = mpsc::UnboundedReceiver<RespawnResult>;

const TICK: Duration = Duration::from_millis(500);
const APP_TITLE: &str = "ShellMusic";

pub(super) enum AppMsg {
    SearchDone(u64, Result<Vec<Track>, String>),
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

/// Opciones de arranque procedentes de la línea de comandos.
pub struct Startup {
    /// Aviso pendiente de mostrar en la línea de estado (yt-dlp antiguo).
    pub warning: Option<String>,
    /// `--play <búsqueda>`: buscar al arrancar y sonar la primera coincidencia.
    pub play: Option<String>,
}

pub async fn run(
    terminal: &mut DefaultTerminal,
    providers: Vec<(ProviderKind, Arc<dyn MusicProvider>)>,
    player: Arc<dyn Player>,
    mut player_events: mpsc::UnboundedReceiver<PlayerEvent>,
    config: Config,
    picker: ratatui_image::picker::Picker,
    startup: Startup,
) -> Result<()> {
    let mut app = App::new(&config)?;
    app.picker = Some(picker);
    if let Some(warning) = startup.warning
        && app.message.is_none()
    {
        app.message = Some(Message::info(warning));
    }
    if !providers.is_empty() {
        app.providers = providers;
        app.active_provider = 0;
    }
    let (tx, mut rx) = mpsc::unbounded_channel::<AppMsg>();
    let mut keys = EventStream::new();

    // `--play <búsqueda>`: buscar al arrancar; la primera coincidencia suena
    // sola en cuanto llegan los resultados (flag consumido en SearchDone).
    if let Some(query) = startup.play {
        app.input = query;
        app.autoplay_first = true;
        playback::submit_search(&mut app, &tx);
    }

    // Reproductor intercambiable: si mpv muere en marcha se respawnea con
    // el volumen vigente. Guarda anti-bucle: máx. 3 respawns en 60 s.
    let youtube = config.youtube.clone();
    let audio_format = config.audio.format.clone();
    let mut player = player;
    let mut respawn_times: Vec<std::time::Instant> = Vec::new();
    let mut respawn_rx: Option<RespawnRx> = None;
    let mut player_respawning = false;

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
                periodic_save(&mut app);
                if !player_respawning
                    && let Ok(status) = player.status().await
                {
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
                PlayerEvent::Died => {
                    if !player_respawning {
                        respawn_times.retain(|t| t.elapsed() < Duration::from_secs(60));
                        if respawn_times.len() >= 3 {
                            app.message = Some(Message::error(
                                "mpv no arranca tras varios intentos; reinicia la app",
                            ));
                        } else {
                            respawn_times.push(std::time::Instant::now());
                            player_respawning = true;
                            app.message = Some(Message::info("mpv se cerró; reiniciándolo…"));
                            respawn_player(&mut respawn_rx, &youtube, &audio_format, app.status.volume);
                        }
                    }
                }
            },
            // El respawn de mpv no es instantáneo: no puede vivir dentro del
            // select! (tokio::spawn devuelve JoinHandle, no un receiver).
            // La tarea comunica el resultado por su propio canal.
            Some(result) = async { match respawn_rx.as_mut() { Some(rx) => rx.recv().await, None => std::future::pending().await } } => {
                player_respawning = false;
                respawn_rx = None;
                match result {
                    Ok((new_player, new_events)) => {
                        player = new_player;
                        player_events = new_events;
                        app.message = Some(Message::info("mpv reiniciado"));
                    }
                    Err(err) => {
                        app.message = Some(Message::error(format!("no se pudo reiniciar mpv: {err}")));
                    }
                }
            }
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

/// Lanza el respawn de mpv en segundo plano; el resultado llega por el canal.
fn respawn_player(
    respawn_rx: &mut Option<RespawnRx>,
    youtube: &crate::config::Youtube,
    audio_format: &str,
    volume: u8,
) {
    let youtube = youtube.clone();
    let audio_format = audio_format.to_string();
    let (done_tx, done_rx) = mpsc::unbounded_channel();
    *respawn_rx = Some(done_rx);
    tokio::spawn(async move {
        let result = crate::player::mpv::MpvPlayer::spawn(volume, &youtube, &audio_format)
            .await
            .map(|(player, events)| (Arc::new(player) as Arc<dyn Player>, events))
            .map_err(|err| err.to_string());
        let _ = done_tx.send(result);
    });
}

/// Guardado periódico de la sesión (posición) y de estadísticas pendientes:
/// acota la pérdida ante un kill -9 a ~30 s en lugar de al último cambio de
/// pista. El guardado propiamente dicho se hace en el tick, aquí solo se
/// decide cuándo.
fn periodic_save(app: &mut App) {
    let now = std::time::Instant::now();
    let due = app
        .last_periodic_save
        .is_none_or(|t| t.elapsed() >= Duration::from_secs(30));
    if due && app.playback_active {
        app.last_periodic_save = Some(now);
        app.save_session(app.status.position);
        if app.stats_dirty {
            app.save_stats();
            app.stats_dirty = false;
        }
    }
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
