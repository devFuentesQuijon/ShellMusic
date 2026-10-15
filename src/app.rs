use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures_util::StreamExt;
use ratatui::DefaultTerminal;
use ratatui::widgets::ListState;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::favorites::Favorites;
use crate::player::{Player, PlaybackStatus, PlayerEvent, PlayerNotice};
use crate::playlists::Playlists;
use crate::providers::{MusicProvider, Track};
use crate::queue::{Queue, Removed};
use crate::tui;
use crate::visualizer::Visualizer;

const TICK: Duration = Duration::from_millis(500);
const VOLUME_STEP: i16 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Searching,
    Browsing,
    Queue,
    Favorites,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Self::Searching => Self::Browsing,
            Self::Browsing => Self::Queue,
            Self::Queue => Self::Favorites,
            Self::Favorites => Self::Searching,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Searching => Self::Favorites,
            Self::Favorites => Self::Queue,
            Self::Queue => Self::Browsing,
            Self::Browsing => Self::Searching,
        }
    }
}

pub struct Message {
    pub text: String,
    pub is_error: bool,
}

impl Message {
    fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: false,
        }
    }

    fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: true,
        }
    }
}

/// Estado del overlay modal de playlists.
#[derive(Debug, Clone)]
pub enum Overlay {
    Closed,
    /// Lista de playlists.
    List,
    /// Contenido de la playlist con ese índice.
    Content(usize),
    /// Input de nombre al crear una playlist.
    NameInput(NameContext),
    /// Selector de playlist para añadir la pista dada.
    Pick(Track),
}

#[derive(Debug, Clone)]
pub enum NameContext {
    /// Creación desde la lista de playlists.
    Plain,
    /// Creación desde el selector de "añadir a playlist": la pista se añade al crear.
    WithTrack(Box<Track>),
}

enum AppMsg {
    SearchDone(Result<Vec<Track>, String>),
    Error(String),
    PlaybackStarted,
}

pub struct App {
    pub focus: Focus,
    pub input: String,
    pub results: Vec<Track>,
    pub list_state: ListState,
    pub queue: Queue,
    pub queue_state: ListState,
    pub favorites: Favorites,
    pub favorites_state: ListState,
    pub playlists: Playlists,
    pub overlay: Overlay,
    pub overlay_list_state: ListState,
    pub overlay_content_state: ListState,
    pub overlay_input: String,
    pub pending_delete: Option<usize>,
    pub status: PlaybackStatus,
    pub message: Option<Message>,
    pub searching: bool,
    pub should_quit: bool,
    pub visualizer: Option<Visualizer>,
    results_limit: usize,
    visualizer_cfg: crate::config::Visualizer,
    favorites_path: std::path::PathBuf,
    playlists_path: std::path::PathBuf,
}

impl App {
    pub fn new(config: &Config) -> Self {
        let data_dir = dirs::data_dir()
            .unwrap_or_default()
            .join("shellmusic");
        Self::with_data_dir(config, &data_dir)
    }

    pub fn with_data_dir(config: &Config, data_dir: &std::path::Path) -> Self {
        let favorites_path = data_dir.join("favorites.json");
        let playlists_path = data_dir.join("playlists.json");
        let (favorites, fav_warning) = Favorites::load(&favorites_path);
        let (playlists, pl_warning) = Playlists::load(&playlists_path);
        let mut app = Self {
            focus: Focus::Searching,
            input: String::new(),
            results: Vec::new(),
            list_state: ListState::default(),
            queue: Queue::new(),
            queue_state: ListState::default(),
            favorites,
            favorites_state: ListState::default(),
            playlists,
            overlay: Overlay::Closed,
            overlay_list_state: ListState::default(),
            overlay_content_state: ListState::default(),
            overlay_input: String::new(),
            pending_delete: None,
            status: PlaybackStatus {
                volume: config.audio.initial_volume,
                ..PlaybackStatus::default()
            },
            message: fav_warning.or(pl_warning).map(Message::error),
            searching: false,
            should_quit: false,
            visualizer: None,
            results_limit: config.general.results_per_search,
            visualizer_cfg: config.visualizer.clone(),
            favorites_path,
            playlists_path,
        };
        if config.visualizer.enabled {
            app.toggle_visualizer();
        }
        if !app.favorites.tracks.is_empty() {
            app.favorites_state.select(Some(0));
        }
        app
    }

    pub fn toggle_visualizer(&mut self) {
        if self.visualizer.is_some() {
            self.visualizer = None;
            self.message = Some(Message::info("visualizador desactivado"));
            return;
        }
        if !cfg!(target_os = "linux") {
            self.message = Some(Message::info(
                "visualizador solo disponible en Linux por ahora",
            ));
            return;
        }
        match Visualizer::start(self.visualizer_cfg.bands) {
            Ok(visualizer) => {
                self.visualizer = Some(visualizer);
                self.message = Some(Message::info("visualizador activado"));
            }
            Err(err) => {
                self.message = Some(Message::error(format!("visualizador no disponible: {err}")));
            }
        }
    }

    pub fn visualizer_fps(&self) -> u16 {
        self.visualizer_cfg.fps
    }

    fn selected(&self) -> Option<usize> {
        self.list_state.selected()
    }

    fn select(&mut self, index: usize) {
        if !self.results.is_empty() {
            self.list_state
                .select(Some(index.min(self.results.len() - 1)));
        }
    }

    fn queue_selected(&self) -> Option<usize> {
        self.queue_state.selected()
    }

    pub fn queue_select(&mut self, index: usize) {
        if !self.queue.is_empty() {
            self.queue_state
                .select(Some(index.min(self.queue.len() - 1)));
        }
    }

    fn favorites_selected(&self) -> Option<usize> {
        self.favorites_state.selected()
    }

    pub fn favorites_select(&mut self, index: usize) {
        if !self.favorites.tracks.is_empty() {
            self.favorites_state
                .select(Some(index.min(self.favorites.tracks.len() - 1)));
        }
    }

    fn save_favorites(&mut self) {
        if let Err(err) = self.favorites.save(&self.favorites_path) {
            self.message = Some(Message::error(format!(
                "no se pudieron guardar los favoritos: {err}"
            )));
        }
    }

    pub fn overlay_list_select(&mut self, index: usize) {
        if !self.playlists.items.is_empty() {
            self.overlay_list_state
                .select(Some(index.min(self.playlists.items.len() - 1)));
        }
    }

    pub fn overlay_content_select(&mut self, playlist: usize, index: usize) {
        let len = self
            .playlists
            .items
            .get(playlist)
            .map_or(0, |p| p.tracks.len());
        if len > 0 {
            self.overlay_content_state
                .select(Some(index.min(len - 1)));
        }
    }

    fn save_playlists(&mut self) {
        if let Err(err) = self.playlists.save(&self.playlists_path) {
            self.message = Some(Message::error(format!(
                "no se pudieron guardar las playlists: {err}"
            )));
        }
    }
}

pub async fn run(
    terminal: &mut DefaultTerminal,
    provider: Arc<dyn MusicProvider>,
    player: Arc<dyn Player>,
    mut player_events: mpsc::UnboundedReceiver<PlayerEvent>,
    config: Config,
) -> Result<()> {
    let mut app = App::new(&config);
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
                    handle_key(&mut app, key, &provider, &player, &tx);
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

fn handle_msg(app: &mut App, msg: AppMsg) {
    match msg {
        AppMsg::SearchDone(Ok(tracks)) => {
            app.searching = false;
            if tracks.is_empty() {
                app.message = Some(Message::info("sin resultados"));
            } else {
                app.message = None;
                app.results = tracks;
                app.select(0);
                app.focus = Focus::Browsing;
            }
        }
        AppMsg::SearchDone(Err(err)) => {
            app.searching = false;
            app.message = Some(Message::error(err));
        }
        AppMsg::Error(err) => app.message = Some(Message::error(err)),
        AppMsg::PlaybackStarted => {
            if app.message.as_ref().is_some_and(|m| !m.is_error) {
                app.message = None;
            }
        }
    }
}

fn handle_key(
    app: &mut App,
    key: KeyEvent,
    provider: &Arc<dyn MusicProvider>,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return;
    }
    // El overlay de playlists es modal: captura todas las teclas salvo Ctrl+C.
    if !matches!(app.overlay, Overlay::Closed) {
        handle_overlay_key(app, key, player, tx);
        return;
    }
    match app.focus {
        Focus::Searching => match key.code {
            KeyCode::Enter => submit_search(app, provider, tx),
            KeyCode::Esc | KeyCode::Tab => app.focus = Focus::Browsing,
            KeyCode::BackTab => app.focus = Focus::Favorites,
            KeyCode::Backspace => {
                app.input.pop();
            }
            KeyCode::Char(c) => app.input.push(c),
            _ => {}
        },
        Focus::Browsing => match key.code {
            KeyCode::Tab => app.focus = app.focus.next(),
            KeyCode::BackTab => app.focus = app.focus.prev(),
            KeyCode::Enter => {
                if let Some(index) = app.selected() {
                    play_result(app, player, tx, index);
                }
            }
            KeyCode::Char('a') => {
                if let Some(index) = app.selected() {
                    add_to_queue(app, app.results[index].clone());
                }
            }
            KeyCode::Char('f') => {
                let track = app
                    .selected()
                    .and_then(|i| app.results.get(i))
                    .or_else(|| app.queue.current_track());
                toggle_favorite(app, track.cloned());
            }
            KeyCode::Char('A') => {
                let track = app
                    .selected()
                    .and_then(|i| app.results.get(i))
                    .or_else(|| app.queue.current_track());
                open_pick_playlist(app, track.cloned());
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(index) = app.selected() {
                    app.select(index.saturating_sub(1));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(index) = app.selected() {
                    app.select(index + 1);
                }
            }
            _ => playback_key(app, key, player, tx),
        },
        Focus::Queue => match key.code {
            KeyCode::Tab => app.focus = app.focus.next(),
            KeyCode::BackTab => app.focus = app.focus.prev(),
            KeyCode::Esc => app.focus = Focus::Browsing,
            KeyCode::Enter => {
                if let Some(index) = app.queue_selected() {
                    play_queue_track(app, player, tx, index);
                }
            }
            KeyCode::Char('d') => remove_from_queue(app, player, tx),
            KeyCode::Char('f') => {
                let track = app
                    .queue_selected()
                    .and_then(|i| app.queue.tracks.get(i))
                    .or_else(|| app.queue.current_track());
                toggle_favorite(app, track.cloned());
            }
            KeyCode::Char('A') => {
                let track = app
                    .queue_selected()
                    .and_then(|i| app.queue.tracks.get(i))
                    .or_else(|| app.queue.current_track());
                open_pick_playlist(app, track.cloned());
            }
            KeyCode::Char('J') => {
                if let Some(index) = app.queue_selected()
                    && app.queue.move_down(index)
                {
                    app.queue_select(index + 1);
                }
            }
            KeyCode::Char('K') => {
                if let Some(index) = app.queue_selected()
                    && app.queue.move_up(index)
                {
                    app.queue_select(index - 1);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(index) = app.queue_selected() {
                    app.queue_select(index.saturating_sub(1));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(index) = app.queue_selected() {
                    app.queue_select(index + 1);
                }
            }
            _ => playback_key(app, key, player, tx),
        },
        Focus::Favorites => match key.code {
            KeyCode::Tab => app.focus = app.focus.next(),
            KeyCode::BackTab => app.focus = app.focus.prev(),
            KeyCode::Esc => app.focus = Focus::Browsing,
            KeyCode::Enter => {
                if let Some(index) = app.favorites_selected() {
                    let track = app.favorites.tracks[index].clone();
                    let queue_index = app.queue.add(track);
                    play_queue_track(app, player, tx, queue_index);
                }
            }
            KeyCode::Char('a') => {
                if let Some(index) = app.favorites_selected() {
                    add_to_queue(app, app.favorites.tracks[index].clone());
                }
            }
            KeyCode::Char('d') | KeyCode::Char('f') => {
                if let Some(index) = app.favorites_selected() {
                    if let Some(track) = app.favorites.remove(index) {
                        app.message =
                            Some(Message::info(format!("quitada de favoritos: {}", track.title)));
                        app.save_favorites();
                    }
                    if app.favorites.tracks.is_empty() {
                        app.favorites_state.select(None);
                    } else {
                        app.favorites_select(index);
                    }
                }
            }
            KeyCode::Char('A') => {
                let track = app
                    .favorites_selected()
                    .and_then(|i| app.favorites.tracks.get(i))
                    .or_else(|| app.queue.current_track());
                open_pick_playlist(app, track.cloned());
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(index) = app.favorites_selected() {
                    app.favorites_select(index.saturating_sub(1));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(index) = app.favorites_selected() {
                    app.favorites_select(index + 1);
                }
            }
            _ => playback_key(app, key, player, tx),
        },
    }
}

/// Teclas de reproducción compartidas por los focos Browsing y Queue.
fn playback_key(
    app: &mut App,
    key: KeyEvent,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('/') => app.focus = Focus::Searching,
        KeyCode::Char(' ') => spawn_player(player, tx, |p| async move { p.toggle_pause().await }),
        KeyCode::Char('s') => {
            app.queue.current = None;
            app.message = Some(Message::info("detenido"));
            spawn_player(player, tx, |p| async move { p.stop().await });
        }
        KeyCode::Char('+') | KeyCode::Char('=') => change_volume(app, player, tx, VOLUME_STEP),
        KeyCode::Char('-') => change_volume(app, player, tx, -VOLUME_STEP),
        KeyCode::Char('n') => play_adjacent(app, player, tx, Queue::next_index, "fin de la cola"),
        KeyCode::Char('p') => play_adjacent(app, player, tx, Queue::prev_index, "inicio de la cola"),
        KeyCode::Char('P') => {
            app.overlay = Overlay::List;
            app.pending_delete = None;
            if app.overlay_list_state.selected().is_none() {
                app.overlay_list_select(0);
            }
        }
        KeyCode::Char('v') => app.toggle_visualizer(),
        _ => {}
    }
}

fn open_pick_playlist(app: &mut App, track: Option<Track>) {
    match track {
        Some(track) => {
            app.overlay = Overlay::Pick(track);
            // La lista incluye "nueva playlist…" aunque no haya playlists.
            app.overlay_list_state.select(Some(0));
        }
        None => app.message = Some(Message::info("nada que añadir a una playlist")),
    }
}

fn handle_overlay_key(
    app: &mut App,
    key: KeyEvent,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
    let overlay = app.overlay.clone();
    match overlay {
        Overlay::Closed => {}
        Overlay::List => overlay_list_key(app, key),
        Overlay::Content(playlist) => overlay_content_key(app, key, player, tx, playlist),
        Overlay::NameInput(context) => overlay_name_key(app, key, context),
        Overlay::Pick(track) => overlay_pick_key(app, key, track),
    }
}

fn overlay_list_key(app: &mut App, key: KeyEvent) {
    if key.code != KeyCode::Char('d') {
        app.pending_delete = None;
    }
    match key.code {
        KeyCode::Esc | KeyCode::Char('P') => app.overlay = Overlay::Closed,
        KeyCode::Char('n') => {
            app.overlay_input.clear();
            app.overlay = Overlay::NameInput(NameContext::Plain);
        }
        KeyCode::Char('d') => {
            if let Some(index) = app.overlay_list_state.selected() {
                if app.pending_delete == Some(index) {
                    if let Some(playlist) = app.playlists.delete(index) {
                        app.message =
                            Some(Message::info(format!("playlist «{}» eliminada", playlist.name)));
                        app.save_playlists();
                    }
                    app.pending_delete = None;
                    if app.playlists.items.is_empty() {
                        app.overlay_list_state.select(None);
                    } else {
                        app.overlay_list_select(index);
                    }
                } else if let Some(playlist) = app.playlists.items.get(index) {
                    app.pending_delete = Some(index);
                    app.message = Some(Message::info(format!(
                        "pulsa d de nuevo para eliminar «{}»",
                        playlist.name
                    )));
                }
            }
        }
        KeyCode::Enter => {
            if let Some(index) = app.overlay_list_state.selected()
                && index < app.playlists.items.len()
            {
                app.overlay = Overlay::Content(index);
                app.overlay_content_select(index, 0);
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(index) = app.overlay_list_state.selected() {
                app.overlay_list_select(index.saturating_sub(1));
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(index) = app.overlay_list_state.selected() {
                app.overlay_list_select(index + 1);
            }
        }
        _ => {}
    }
}

fn overlay_content_key(
    app: &mut App,
    key: KeyEvent,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    playlist: usize,
) {
    let selected = app.overlay_content_state.selected();
    match key.code {
        KeyCode::Esc => app.overlay = Overlay::List,
        KeyCode::Enter => {
            if let Some(index) = selected
                && let Some(track) = app.playlists.items.get(playlist).and_then(|p| p.tracks.get(index)).cloned()
            {
                // El overlay permanece abierto para seguir gestionando la playlist.
                let queue_index = app.queue.add(track);
                play_queue_track(app, player, tx, queue_index);
            }
        }
        KeyCode::Char('a') => {
            if let Some(index) = selected
                && let Some(track) = app.playlists.items.get(playlist).and_then(|p| p.tracks.get(index)).cloned()
            {
                add_to_queue(app, track);
            }
        }
        // Reproducir la playlist entera: encola todo y suena la primera.
        KeyCode::Char('A') => {
            if let Some(tracks) = app.playlists.items.get(playlist).map(|p| p.tracks.clone())
                && !tracks.is_empty()
            {
                let name = app.playlists.items[playlist].name.clone();
                let mut first = None;
                for track in tracks {
                    let index = app.queue.add(track);
                    first.get_or_insert(index);
                }
                if let Some(first) = first {
                    app.overlay = Overlay::Closed;
                    app.message =
                        Some(Message::info(format!("reproduciendo playlist «{name}»")));
                    play_queue_track(app, player, tx, first);
                }
            }
        }
        KeyCode::Char('d') => {
            if let Some(index) = selected
                && let Some(track) = app.playlists.remove_track(playlist, index)
            {
                app.message = Some(Message::info(format!(
                    "quitada de la playlist: {}",
                    track.title
                )));
                app.save_playlists();
                app.overlay_content_select(playlist, index);
            }
        }
        KeyCode::Char('J') => {
            if let Some(index) = selected
                && app.playlists.move_track_down(playlist, index)
            {
                app.overlay_content_select(playlist, index + 1);
                app.save_playlists();
            }
        }
        KeyCode::Char('K') => {
            if let Some(index) = selected
                && app.playlists.move_track_up(playlist, index)
            {
                app.overlay_content_select(playlist, index - 1);
                app.save_playlists();
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(index) = selected {
                app.overlay_content_select(playlist, index.saturating_sub(1));
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(index) = selected {
                app.overlay_content_select(playlist, index + 1);
            }
        }
        _ => {}
    }
}

fn overlay_name_key(app: &mut App, key: KeyEvent, context: NameContext) {
    match key.code {
        KeyCode::Esc => {
            app.overlay = match context {
                NameContext::Plain => Overlay::List,
                NameContext::WithTrack(track) => Overlay::Pick(*track),
            };
        }
        KeyCode::Enter => {
            let name = app.overlay_input.clone();
            match app.playlists.create(&name) {
                Ok(index) => {
                    let created = app.playlists.items[index].name.clone();
                    app.save_playlists();
                    match context {
                        NameContext::Plain => {
                            app.message =
                                Some(Message::info(format!("playlist «{created}» creada")));
                            app.overlay = Overlay::List;
                            app.overlay_list_select(index);
                        }
                        NameContext::WithTrack(track) => {
                            app.playlists.add_track(index, *track.clone());
                            app.save_playlists();
                            app.message = Some(Message::info(format!(
                                "añadida a «{created}»: {}",
                                track.title
                            )));
                            app.overlay = Overlay::Closed;
                        }
                    }
                }
                Err(err) => app.message = Some(Message::error(err)),
            }
        }
        KeyCode::Backspace => {
            app.overlay_input.pop();
        }
        KeyCode::Char(c) => app.overlay_input.push(c),
        _ => {}
    }
}

fn overlay_pick_key(app: &mut App, key: KeyEvent, track: Track) {
    let count = app.playlists.items.len(); // +1 por la entrada "nueva playlist…"
    match key.code {
        KeyCode::Esc => app.overlay = Overlay::Closed,
        KeyCode::Enter => {
            if let Some(index) = app.overlay_list_state.selected() {
                if index >= count {
                    app.overlay_input.clear();
                    app.overlay = Overlay::NameInput(NameContext::WithTrack(Box::new(track)));
                } else {
                    let name = app.playlists.items[index].name.clone();
                    if app.playlists.add_track(index, track.clone()) {
                        app.message =
                            Some(Message::info(format!("añadida a «{name}»: {}", track.title)));
                    } else {
                        app.message =
                            Some(Message::info(format!("ya está en «{name}»: {}", track.title)));
                    }
                    app.save_playlists();
                    app.overlay = Overlay::Closed;
                }
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(index) = app.overlay_list_state.selected() {
                app.overlay_list_state.select(Some(index.saturating_sub(1)));
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(index) = app.overlay_list_state.selected() {
                app.overlay_list_state
                    .select(Some((index + 1).min(count))); // count = índice de "nueva playlist…"
            }
        }
        _ => {}
    }
}

fn add_to_queue(app: &mut App, track: Track) {
    app.queue.add(track.clone());
    app.message = Some(Message::info(format!("añadida a la cola: {}", track.title)));
    if app.queue_selected().is_none() {
        app.queue_select(0);
    }
}

fn toggle_favorite(app: &mut App, track: Option<Track>) {
    let Some(track) = track else {
        app.message = Some(Message::info("nada que marcar como favorito"));
        return;
    };
    if app.favorites.toggle(&track) {
        app.message = Some(Message::info(format!("★ añadida a favoritos: {}", track.title)));
        if app.favorites_selected().is_none() {
            app.favorites_select(0);
        }
    } else {
        app.message = Some(Message::info(format!("quitada de favoritos: {}", track.title)));
    }
    app.save_favorites();
}

fn submit_search(
    app: &mut App,
    provider: &Arc<dyn MusicProvider>,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
    let query = app.input.trim().to_string();
    if query.is_empty() || app.searching {
        return;
    }
    app.searching = true;
    app.message = Some(Message::info(format!("buscando «{query}»…")));
    let provider = Arc::clone(provider);
    let tx = tx.clone();
    let limit = app.results_limit;
    tokio::spawn(async move {
        let result = provider.search(&query, limit).await.map_err(|e| e.to_string());
        let _ = tx.send(AppMsg::SearchDone(result));
    });
}

/// Enter en Resultados: añade a la cola si falta y reproduce al instante.
fn play_result(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    result_index: usize,
) {
    if result_index >= app.results.len() {
        return;
    }
    let track = app.results[result_index].clone();
    let index = app.queue.add(track);
    play_queue_track(app, player, tx, index);
}

fn play_queue_track(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    index: usize,
) {
    if index >= app.queue.len() {
        return;
    }
    app.queue.current = Some(index);
    app.queue_select(index);
    app.message = Some(Message::info(format!(
        "cargando «{}»…",
        app.queue.tracks[index].title
    )));
    let url = app.queue.tracks[index].source_url.clone();
    let player = Arc::clone(player);
    let tx = tx.clone();
    tokio::spawn(async move {
        match player.play(&url).await {
            Ok(()) => {
                let _ = tx.send(AppMsg::PlaybackStarted);
            }
            Err(err) => {
                let _ = tx.send(AppMsg::Error(format!("no se pudo reproducir: {err}")));
            }
        }
    });
}

fn play_adjacent(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    neighbor: fn(&Queue) -> Option<usize>,
    at_edge: &str,
) {
    if app.queue.is_empty() {
        app.message = Some(Message::info("la cola está vacía"));
        return;
    }
    match neighbor(&app.queue) {
        Some(index) => play_queue_track(app, player, tx, index),
        None => app.message = Some(Message::info(at_edge)),
    }
}

fn autoplay_next(app: &mut App, player: &Arc<dyn Player>, tx: &mpsc::UnboundedSender<AppMsg>) {
    if let Some(index) = app.queue.next_index()
        && app.queue.current.is_some()
    {
        play_queue_track(app, player, tx, index);
    }
}

fn remove_from_queue(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
    let Some(index) = app.queue_selected() else {
        return;
    };
    match app.queue.remove(index) {
        Some(Removed::CurrentWithSuccessor(successor)) => {
            app.message = Some(Message::info("eliminada; saltando a la siguiente"));
            play_queue_track(app, player, tx, successor);
        }
        Some(Removed::CurrentWithoutSuccessor) => {
            app.message = Some(Message::info("eliminada la pista que sonaba; detenido"));
            spawn_player(player, tx, |p| async move { p.stop().await });
        }
        Some(Removed::Other) => {
            app.message = Some(Message::info("eliminada de la cola"));
        }
        None => return,
    }
    if app.queue.is_empty() {
        app.queue_state.select(None);
    } else {
        app.queue_select(index);
    }
}

fn change_volume(
    app: &mut App,
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    delta: i16,
) {
    let volume = (i16::from(app.status.volume) + delta).clamp(0, 100) as u8;
    app.status.volume = volume;
    app.message = Some(Message::info(format!("volumen: {volume}")));
    let player = Arc::clone(player);
    let tx = tx.clone();
    tokio::spawn(async move {
        if let Err(err) = player.set_volume(volume).await {
            let _ = tx.send(AppMsg::Error(format!("no se pudo ajustar el volumen: {err}")));
        }
    });
}

fn spawn_player<F, Fut>(
    player: &Arc<dyn Player>,
    tx: &mpsc::UnboundedSender<AppMsg>,
    action: F,
) where
    F: FnOnce(Arc<dyn Player>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<()>> + Send,
{
    let player = Arc::clone(player);
    let tx = tx.clone();
    tokio::spawn(async move {
        if let Err(err) = action(player).await {
            let _ = tx.send(AppMsg::Error(format!("error del reproductor: {err}")));
        }
    });
}
