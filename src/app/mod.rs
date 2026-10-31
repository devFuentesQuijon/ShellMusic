use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyEventKind};
use crossterm::terminal::SetTitle;
use futures_util::StreamExt;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::control::ControlRequest;
use crate::player::{Player, PlayerEvent, PlayerNotice};
use crate::providers::{MusicProvider, ProviderKind, Track};
use crate::system::{MediaCommand, SystemIntegration};
use crate::tui;

pub(crate) mod commands;
mod control;
mod input;
mod playback;
mod state;

pub use state::{
    App, FilterState, Focus, Message, NameContext, Overlay, PanelAreas, PathAction, RadioMode,
    RepeatMode, SleepTimer, StatsView, UndoAction,
};

use input::{handle_key, handle_mouse};
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
    LyricsDone(Result<crate::lyrics::Lyrics, String>),
    CoverDone(Result<CoverImage, CoverError>),
}

/// Carátula descargada y decodificada, lista para convertir en protocolo.
pub struct CoverImage {
    pub track: (ProviderKind, String),
    pub image: image::DynamicImage,
}

/// Fallo de descarga de carátula. La clave de pista permite resolver la
/// notificación pendiente (mandarla sin imagen) en la fase 21.
pub struct CoverError {
    pub message: String,
    pub track: (ProviderKind, String),
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
    let gapless = config.audio.gapless;
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

    // Socket de control para agentes de IA (RF-63): opt-in `[mcp] enabled`.
    // Si no se pudo crear (otra instancia activa, sin permisos) la app sigue
    // funcionando sin control; el puente MCP informará de ello al conectar.
    let mut control_rx: Option<mpsc::UnboundedReceiver<ControlRequest>> = None;
    if config.mcp.enabled {
        let (control_tx, rx) = mpsc::unbounded_channel::<ControlRequest>();
        match crate::control::spawn_server(control_tx).await {
            Ok(()) => control_rx = Some(rx),
            Err(err) => {
                app.message = Some(Message::info(format!(
                    "control para agentes desactivado: {err:#}"
                )));
            }
        }
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

    // Primer frame inmediato; después, redraw perezoso: solo se repinta
    // cuando algo cambia. El tick repinta solo si el estado del player
    // difiere del último frame dibujado, así la app en pausa no renderiza.
    terminal.draw(|frame| tui::draw(frame, &mut app))?;
    let mut needs_redraw = false;
    loop {
        let tick = tokio::time::sleep(TICK);
        tokio::select! {
            event = keys.next() => {
                match event {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        handle_key(&mut app, key, &player, &tx);
                        needs_redraw = true;
                    }
                    Some(Ok(Event::Mouse(mouse))) => {
                        // Solo se repinta si el ratón cambió algo: el modo
                        // «any event» de crossterm reporta movimientos y
                        // repintar en cada uno sería una tormenta de renders.
                        needs_redraw = handle_mouse(&mut app, mouse, &player, &tx);
                    }
                    _ => {}
                }
            }
            _ = tick => {
                playback::release_stale_control_pending(&mut app);
                periodic_save(&mut app);
                // Los mensajes informativos son toasts: caducan y la línea de
                // estado vuelve a la ayuda contextual. Los errores persisten.
                if app
                    .message
                    .as_ref()
                    .is_some_and(|m| !m.is_error && m.created.elapsed() >= state::MESSAGE_TTL)
                {
                    app.message = None;
                    needs_redraw = true;
                }
                // Temporizador de apagado: al expirar detiene la reproducción.
                // El aviso va después de parar: `stop_playback` pisa el mensaje.
                if app
                    .sleep_timer
                    .is_some_and(|timer| matches!(timer, SleepTimer::At(t) if std::time::Instant::now() >= t))
                {
                    app.sleep_timer = None;
                    playback::stop_playback(&mut app, &player, &tx);
                    app.message = Some(Message::info("sleep timer: deteniendo la reproducción"));
                    needs_redraw = true;
                }
                if !player_respawning
                    && let Ok(status) = player.status().await
                {
                    // Puntos máximos de escucha para el scrobble de
                    // Last.fm: solo cuando suena (la pausa no cuenta).
                    if !status.paused
                        && let Some(pos) = status.position
                        && pos > app.playing_peak
                    {
                        app.playing_peak = pos;
                    }
                    if status != app.status {
                        app.status = status;
                        needs_redraw = true;
                    }
                }
            }
            Some(msg) = rx.recv() => {
                handle_msg(&mut app, msg, &player, &tx);
                needs_redraw = true;
            }
            Some(command) = media_rx.recv() => {
                handle_media_command(&mut app, command, &player, &tx);
                needs_redraw = true;
            }
            // Peticiones del socket de control (agentes de IA). La respuesta
            // viaja por el oneshot del propio request: inmediata, o diferida
            // al SearchDone para las que implican búsqueda.
            Some(request) = async { match control_rx.as_mut() { Some(rx) => rx.recv().await, None => std::future::pending().await } } => {
                control::handle(&mut app, &player, &tx, request);
                needs_redraw = true;
            }
            Some(event) = player_events.recv() => {
                match event {
                    PlayerEvent::Notice(PlayerNotice { text, is_error }) => {
                        app.message = Some(Message {
                            text,
                            is_error,
                            created: std::time::Instant::now(),
                        });
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
                                respawn_player(&mut respawn_rx, &youtube, &audio_format, gapless, app.status.volume);
                            }
                        }
                    }
                }
                needs_redraw = true;
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
                        // El mpv nuevo tiene la playlist interna vacía: la
                        // pista apilada para gapless ya no existe.
                        app.mpv_appended = None;
                        app.message = Some(Message::info("mpv reiniciado"));
                    }
                    Err(err) => {
                        app.message = Some(Message::error(format!("no se pudo reiniciar mpv: {err}")));
                    }
                }
                needs_redraw = true;
            }
        }
        if needs_redraw {
            terminal.draw(|frame| tui::draw(frame, &mut app))?;
            needs_redraw = false;
        }
        set_tab_title(&desired_title(&app), &mut last_title);
        system.update(&mut app);
        if app.should_quit {
            break;
        }
    }
    // Scrobble de la última pista antes de guardar la sesión.
    playback::scrobble_leaving(&mut app, &tx);
    app.save_session(app.status.position);
    set_tab_title("", &mut last_title);
    Ok(())
}

/// Lanza el respawn de mpv en segundo plano; el resultado llega por el canal.
fn respawn_player(
    respawn_rx: &mut Option<RespawnRx>,
    youtube: &crate::config::Youtube,
    audio_format: &str,
    gapless: bool,
    volume: u8,
) {
    let youtube = youtube.clone();
    let audio_format = audio_format.to_string();
    let (done_tx, done_rx) = mpsc::unbounded_channel();
    *respawn_rx = Some(done_rx);
    tokio::spawn(async move {
        let result = crate::player::mpv::MpvPlayer::spawn(volume, &youtube, &audio_format, gapless)
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
    use crate::queue::Queue;
    use playback::{change_volume, play_adjacent, seek_by, seek_to, spawn_player};
    match command {
        MediaCommand::Play | MediaCommand::Pause | MediaCommand::Toggle => {
            spawn_player(player, tx, |p| async move { p.toggle_pause().await });
        }
        MediaCommand::Stop => {
            playback::stop_playback(app, player, tx);
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
            seek_by(
                app,
                player,
                tx,
                -(delta.as_secs().min(i32::MAX as u64) as i32),
            );
        }
        MediaCommand::SetPosition(target) => {
            seek_to(app, player, tx, target);
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
            app.message.as_ref().is_some_and(|m| m.text.contains("+30")),
            "seek +30 esperado"
        );
    }
}
