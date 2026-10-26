use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Context;
use ratatui::widgets::ListState;

use crate::config::Config;
use crate::favorites::Favorites;
use crate::history::History;
use crate::player::PlaybackStatus;
use crate::playlists::Playlists;
use crate::providers::youtube::YoutubeProvider;
use crate::providers::{MusicProvider, ProviderKind, Track};
use crate::queue::Queue;
use crate::session::Session;
use crate::stats::Stats;
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Searching,
    Browsing,
    Queue,
    Favorites,
}

impl Focus {
    pub(super) fn next(self) -> Self {
        match self {
            Self::Searching => Self::Browsing,
            Self::Browsing => Self::Queue,
            Self::Queue => Self::Favorites,
            Self::Favorites => Self::Searching,
        }
    }

    pub(super) fn prev(self) -> Self {
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
    pub(super) fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: false,
        }
    }

    pub(super) fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: true,
        }
    }
}

/// Sub-modo de la radio: de dónde salen las pistas al agotarse la cola.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RadioMode {
    /// Pistas del último artista sonado.
    #[default]
    Artista,
    /// Similares a la última pista (mix RD de YouTube; en SoundCloud,
    /// búsqueda por artista + título).
    Similares,
    /// Artista al azar de los favoritos (o género al azar si no hay).
    Random,
    /// Palabra clave de género/década al azar.
    RandomGenero,
    /// Pistas menos conocidas del último artista (salta los hits).
    Rarezas,
}

impl RadioMode {
    pub fn next(self) -> Self {
        match self {
            Self::Artista => Self::Similares,
            Self::Similares => Self::Random,
            Self::Random => Self::RandomGenero,
            Self::RandomGenero => Self::Rarezas,
            Self::Rarezas => Self::Artista,
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "artista" => Some(Self::Artista),
            "similares" => Some(Self::Similares),
            "random" => Some(Self::Random),
            "random-genero" => Some(Self::RandomGenero),
            "rarezas" => Some(Self::Rarezas),
            _ => None,
        }
    }
}

impl std::fmt::Display for RadioMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Artista => "artista",
            Self::Similares => "similares",
            Self::Random => "random",
            Self::RandomGenero => "random género",
            Self::Rarezas => "rarezas",
        };
        write!(f, "{name}")
    }
}

/// Modo de repetición al terminar una pista de forma natural.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepeatMode {
    /// Al acabar la cola, se detiene (o entra la radio si está activa).
    #[default]
    Off,
    /// Al llegar al final de la cola, vuelve a empezar por la primera.
    All,
    /// Repite la pista actual en bucle.
    One,
}

impl RepeatMode {
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }

    /// Etiqueta corta para la barra del reproductor; vacía si está inactivo.
    pub fn bar_label(self) -> &'static str {
        match self {
            Self::Off => "",
            Self::All => "  rep:all",
            Self::One => "  rep:1",
        }
    }
}

impl std::fmt::Display for RepeatMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Off => "desactivada",
            Self::All => "toda la cola",
            Self::One => "pista actual",
        };
        write!(f, "{name}")
    }
}

/// Estado del overlay modal.
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
    /// Ayuda contextual de atajos de teclado.
    Help,
    /// Estadísticas de escucha ("tus más escuchadas").
    Stats,
    /// Letra de una pista (lrclib.net).
    Lyrics,
    /// Panel «sonando ahora» con metadatos ampliados y carátula.
    NowPlaying,
    /// Menú de exportación JSON (qué incluir).
    Export,
    /// Input de ruta para exportar o importar.
    PathInput(PathAction),
}

/// Acción que ejecuta `Overlay::PathInput` al confirmar la ruta.
#[derive(Debug, Clone)]
pub enum PathAction {
    /// Copia de seguridad JSON con el ámbito elegido.
    ExportJson(crate::portable::ExportScope),
    /// Exportar la playlist con ese índice a M3U.
    ExportM3u(usize),
    /// Importar desde JSON o M3U según la extensión.
    Import,
}

#[derive(Debug, Clone)]
pub enum NameContext {
    /// Creación desde la lista de playlists.
    Plain,
    /// Creación desde el selector de "añadir a playlist": la pista se añade al crear.
    WithTrack(Box<Track>),
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
    /// Filas visibles de los paneles de listas (resultados/cola/favoritos):
    /// la fija el render en cada frame; alimenta los saltos PgUp/PgDn.
    pub list_page_size: usize,
    /// Filas visibles de las listas del overlay de playlists.
    pub overlay_page_size: usize,
    pub overlay_input: String,
    pub pending_delete: Option<usize>,
    pub status: PlaybackStatus,
    pub message: Option<Message>,
    pub searching: bool,
    pub should_quit: bool,
    pub providers: Vec<(ProviderKind, Arc<dyn MusicProvider>)>,
    pub active_provider: usize,
    pub radio: bool,
    pub radio_mode: RadioMode,
    pub repeat: RepeatMode,
    pub played: HashSet<(ProviderKind, String)>,
    /// Títulos normalizados ya reproducidos: la radio no encola otra versión
    /// de la misma canción.
    pub played_titles: HashSet<String>,
    pub radio_epoch: u64,
    /// Pista para la que ya se pidió expansión de radio: evita relanzar la
    /// búsqueda en cada play y martillear yt-dlp tras un fallo.
    pub radio_expanded_for: Option<(ProviderKind, String)>,
    /// true mientras hay reproducción en curso; RadioDone solo auto-reproduce
    /// si el reproductor está parado (cola agotada), no en prefetch.
    pub playback_active: bool,
    pub(super) results_limit: usize,
    favorites_path: std::path::PathBuf,
    playlists_path: std::path::PathBuf,
    session_path: std::path::PathBuf,
    /// Tema de color resuelto de `[theme]` en la config.
    pub theme: Theme,
    /// Posición desde la que reanudar la pista restaurada; la consume
    /// `app::run` al arrancar.
    pub pending_position: Option<std::time::Duration>,
    pub history: History,
    /// Posición al navegar el historial con ↑/↓; None = editando texto nuevo.
    history_index: Option<usize>,
    /// Borrador del input al empezar a navegar el historial; se restaura al
    /// bajar más allá de la búsqueda más reciente.
    history_draft: String,
    history_path: std::path::PathBuf,
    pub stats: Stats,
    stats_path: std::path::PathBuf,
    /// Vista del overlay de estadísticas: false = pistas, true = artistas.
    pub stats_by_artist: bool,
    /// Selección dentro del overlay de estadísticas.
    pub stats_state: ListState,
    /// Cache de la vista pistas ordenada (índice → pista) para Enter/a.
    pub stats_top: Vec<Track>,
    /// Texto de la letra en el overlay (None = cargando).
    pub lyrics_text: Option<String>,
    /// Título de la pista de la letra cargada/pedida.
    pub lyrics_title: String,
    /// Scroll del overlay de letras (líneas).
    pub lyrics_scroll: u16,
    /// Detector de protocolo de imagen del terminal (kitty/sixel/halfblocks).
    pub picker: Option<ratatui_image::picker::Picker>,
    /// Carátula lista para dibujar en el panel «sonando ahora».
    pub cover: Option<ratatui_image::protocol::StatefulProtocol>,
    /// Pista (provider, id) de la carátula cacheada.
    pub cover_for: Option<(ProviderKind, String)>,
}

impl App {
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        let data_dir = dirs::data_dir()
            .unwrap_or_default()
            .join("shellmusic");
        Self::with_data_dir(config, &data_dir)
    }

    pub fn with_data_dir(config: &Config, data_dir: &std::path::Path) -> anyhow::Result<Self> {
        let favorites_path = data_dir.join("favorites.json");
        let playlists_path = data_dir.join("playlists.json");
        let history_path = data_dir.join("history.json");
        let session_path = data_dir.join("session.json");
        let stats_path = data_dir.join("stats.json");
        let (favorites, fav_warning) = Favorites::load(&favorites_path)
            .with_context(|| format!("no se pudieron leer {}", favorites_path.display()))?;
        let (playlists, pl_warning) = Playlists::load(&playlists_path)
            .with_context(|| format!("no se pudieron leer {}", playlists_path.display()))?;
        let (history, hist_warning) = History::load(&history_path)
            .with_context(|| format!("no se pudo leer {}", history_path.display()))?;
        let (session, sess_warning) = Session::load(&session_path)
            .with_context(|| format!("no se pudo leer {}", session_path.display()))?;
        let (stats, stats_warning) = Stats::load(&stats_path)
            .with_context(|| format!("no se pudieron leer {}", stats_path.display()))?;
        let (theme, theme_warning) = Theme::resolve(&config.theme);
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
            list_page_size: 10,
            overlay_page_size: 10,
            overlay_input: String::new(),
            pending_delete: None,
            status: PlaybackStatus {
                volume: config.audio.initial_volume,
                ..PlaybackStatus::default()
            },
            message: fav_warning
                .or(pl_warning)
                .or(hist_warning)
                .or(sess_warning)
                .or(stats_warning)
                .or(theme_warning)
                .map(Message::error),
            searching: false,
            should_quit: false,
            providers: vec![(ProviderKind::Youtube, Arc::new(YoutubeProvider::new()))],
            active_provider: 0,
            radio: config.radio.enabled,
            radio_mode: RadioMode::from_name(&config.radio.mode).unwrap_or_default(),
            repeat: RepeatMode::default(),
            played: HashSet::new(),
            played_titles: HashSet::new(),
            radio_epoch: 0,
            radio_expanded_for: None,
            playback_active: false,
            results_limit: config.general.results_per_search,
            favorites_path,
            playlists_path,
            session_path,
            theme,
            pending_position: session.position,
            history,
            history_index: None,
            history_draft: String::new(),
            history_path,
            stats,
            stats_path,
            stats_by_artist: false,
            stats_state: ListState::default(),
            stats_top: Vec::new(),
            lyrics_text: None,
            lyrics_title: String::new(),
            lyrics_scroll: 0,
            picker: None,
            cover: None,
            cover_for: None,
        };
        if !app.favorites.tracks.is_empty() {
            app.favorites_state.select(Some(0));
        }
        // Cola persistente: restaura pistas y la que sonaba (con su posición,
        // que `app::run` consumirá para reanudarla).
        app.queue.tracks = session.tracks;
        app.queue.current = session.current;
        if let Some(current) = session.current {
            app.queue_select(current);
        } else if !app.queue.is_empty() {
            app.queue_select(0);
        }
        Ok(app)
    }

    pub fn active_provider_kind(&self) -> ProviderKind {
        self.providers
            .get(self.active_provider)
            .map_or(ProviderKind::Youtube, |(kind, _)| *kind)
    }

    pub fn next_provider(&mut self) {
        if self.providers.is_empty() {
            return;
        }
        self.active_provider = (self.active_provider + 1) % self.providers.len();
        self.message = Some(Message::info(format!(
            "Proveedor: {}",
            self.providers[self.active_provider].0
        )));
    }

    pub fn toggle_radio(&mut self) {
        self.radio = !self.radio;
        self.radio_epoch += 1;
        self.message = Some(Message::info(if self.radio {
            "Modo radio activado"
        } else {
            "Modo radio desactivado"
        }));
    }

    /// Cicla el sub-modo de la radio (tecla R).
    pub fn next_radio_mode(&mut self) {
        self.radio_mode = self.radio_mode.next();
        let note = if self.radio { "" } else { " (radio inactiva: actívala con r)" };
        self.message = Some(Message::info(format!(
            "Modo radio: {}{note}",
            self.radio_mode
        )));
    }

    /// Cicla el modo de repetición (tecla l): off → toda la cola → pista actual.
    pub fn next_repeat_mode(&mut self) {
        self.repeat = self.repeat.next();
        self.message = Some(Message::info(format!("Repetición: {}", self.repeat)));
    }

    pub(super) fn selected(&self) -> Option<usize> {
        self.list_state.selected()
    }

    pub(super) fn select(&mut self, index: usize) {
        if !self.results.is_empty() {
            self.list_state
                .select(Some(index.min(self.results.len() - 1)));
        }
    }

    pub(super) fn queue_selected(&self) -> Option<usize> {
        self.queue_state.selected()
    }

    pub fn queue_select(&mut self, index: usize) {
        if !self.queue.is_empty() {
            self.queue_state
                .select(Some(index.min(self.queue.len() - 1)));
        }
    }

    pub(super) fn favorites_selected(&self) -> Option<usize> {
        self.favorites_state.selected()
    }

    pub fn favorites_select(&mut self, index: usize) {
        if !self.favorites.tracks.is_empty() {
            self.favorites_state
                .select(Some(index.min(self.favorites.tracks.len() - 1)));
        }
    }

    pub(super) fn save_favorites(&mut self) {
        if let Err(err) = self.favorites.save(&self.favorites_path) {
            self.message = Some(Message::error(format!(
                "no se pudieron guardar los favoritos: {err}"
            )));
        }
    }

    /// Persiste la cola actual (pistas, pista en curso y `position`) para
    /// restaurarla en el próximo arranque.
    pub fn save_session(&mut self, position: Option<std::time::Duration>) {
        let session = Session {
            tracks: self.queue.tracks.clone(),
            current: self.queue.current,
            position,
        };
        if let Err(err) = session.save(&self.session_path) {
            self.message = Some(Message::error(format!(
                "no se pudo guardar la sesión: {err}"
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

    pub(super) fn save_playlists(&mut self) {
        if let Err(err) = self.playlists.save(&self.playlists_path) {
            self.message = Some(Message::error(format!(
                "no se pudieron guardar las playlists: {err}"
            )));
        }
    }

    /// ↑ en la caja de búsqueda: baja por el historial hacia lo más antiguo.
    pub fn history_prev(&mut self) {
        if self.history.entries.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                self.history_draft = self.input.clone();
                self.history_index = Some(self.history.entries.len() - 1);
            }
            Some(0) => return,
            Some(index) => self.history_index = Some(index - 1),
        }
        if let Some(index) = self.history_index {
            self.input = self.history.entries[index].clone();
        }
    }

    /// ↓ en la caja de búsqueda: sube hacia lo más reciente; al pasar de la
    /// última entrada restaura el borrador que se estaba escribiendo.
    pub fn history_next(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };
        if index + 1 < self.history.entries.len() {
            self.history_index = Some(index + 1);
            self.input = self.history.entries[index + 1].clone();
        } else {
            self.history_index = None;
            self.input = self.history_draft.clone();
        }
    }

    /// Sugerencia de autocompletado: la entrada más reciente del historial
    /// que empiece por el input actual (y sea más larga). No hay sugerencia
    /// con input vacío ni mientras se navega el historial con ↑/↓.
    pub fn history_suggestion(&self) -> Option<&str> {
        if self.input.is_empty() || self.history_index.is_some() {
            return None;
        }
        self.history
            .entries
            .iter()
            .rev()
            .find(|entry| entry.len() > self.input.len() && entry.starts_with(self.input.as_str()))
            .map(String::as_str)
    }

    /// Acepta la sugerencia de autocompletado (tecla → en la búsqueda).
    pub fn accept_suggestion(&mut self) {
        if let Some(suggestion) = self.history_suggestion().map(str::to_string) {
            self.input = suggestion;
            self.history_reset();
        }
    }

    /// Sale de la navegación del historial (al cambiar de foco o buscar).
    pub(super) fn history_reset(&mut self) {
        self.history_index = None;
        self.history_draft.clear();
    }

    pub(super) fn save_history(&mut self) {
        if let Err(err) = self.history.save(&self.history_path) {
            self.message = Some(Message::error(format!(
                "no se pudo guardar el historial: {err}"
            )));
        }
    }

    pub fn save_stats(&mut self) {
        if let Err(err) = self.stats.save(&self.stats_path) {
            self.message = Some(Message::error(format!(
                "no se pudieron guardar las estadísticas: {err}"
            )));
        }
    }

    /// Abre el overlay de estadísticas con la vista de pistas ordenada.
    pub fn open_stats(&mut self) {
        self.refresh_stats_view();
        self.overlay = Overlay::Stats;
        if self.stats_state.selected().is_none() && !self.stats_top.is_empty() {
            self.stats_state.select(Some(0));
        }
    }

    /// Reordena la vista de pistas por reproducciones (tras abrir o grabar).
    pub fn refresh_stats_view(&mut self) {
        self.stats_top = self
            .stats
            .top_tracks()
            .into_iter()
            .map(|(_, track)| track)
            .collect();
    }

    pub fn stats_select(&mut self, index: usize) {
        let len = if self.stats_by_artist {
            self.stats.top_artists().len()
        } else {
            self.stats_top.len()
        };
        if len > 0 {
            self.stats_state.select(Some(index.min(len - 1)));
        }
    }

    /// Ejecuta la acción de la ruta confirmada en el overlay PathInput.
    pub fn run_path_action(&mut self, action: &PathAction) {
        let input = self.overlay_input.clone();
        let path = match crate::portable::expand_path(&input) {
            Ok(path) => path,
            Err(err) => {
                self.message = Some(Message::error(err));
                return;
            }
        };
        match action {
            PathAction::ExportJson(scope) => {
                let text =
                    crate::portable::export_json(*scope, &self.favorites.tracks, &self.playlists.items);
                match crate::portable::write_export(&path, &text) {
                    Ok(()) => {
                        self.message =
                            Some(Message::info(format!("exportado a {}", path.display())));
                        self.overlay = Overlay::Closed;
                    }
                    Err(err) => {
                        self.message = Some(Message::error(format!(
                            "no se pudo exportar: {err}"
                        )));
                    }
                }
            }
            PathAction::ExportM3u(playlist) => {
                let Some(playlist) = self.playlists.items.get(*playlist) else {
                    self.message = Some(Message::error("la playlist ya no existe"));
                    self.overlay = Overlay::Closed;
                    return;
                };
                let text = crate::portable::export_m3u(playlist);
                match crate::portable::write_export(&path, &text) {
                    Ok(()) => {
                        self.message =
                            Some(Message::info(format!("exportado a {}", path.display())));
                        self.overlay = Overlay::Content(self.playlist_index_of(playlist));
                    }
                    Err(err) => {
                        self.message = Some(Message::error(format!(
                            "no se pudo exportar: {err}"
                        )));
                    }
                }
            }
            PathAction::Import => self.import_path(&path),
        }
    }

    fn playlist_index_of(&self, playlist: &crate::playlists::Playlist) -> usize {
        self.playlists
            .items
            .iter()
            .position(|p| p.name == playlist.name)
            .unwrap_or(0)
    }

    fn import_path(&mut self, path: &std::path::Path) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) => {
                self.message = Some(Message::error(format!(
                    "no se pudo leer {}: {err}",
                    path.display()
                )));
                return;
            }
        };
        let result = if ext == "m3u" || ext == "m3u8" {
            self.import_m3u_text(path, &text)
        } else {
            self.import_json_text(&text)
        };
        match result {
            Ok(summary) => {
                self.message = Some(Message::info(summary));
                self.overlay = Overlay::Closed;
            }
            Err(err) => self.message = Some(Message::error(err)),
        }
    }

    fn import_json_text(&mut self, text: &str) -> Result<String, String> {
        let file = crate::portable::import_json(text)?;
        let mut favs = 0;
        for track in file.favorites {
            if self.favorites.add(track) {
                favs += 1;
            }
        }
        let mut playlists_new = 0;
        let mut tracks_new = 0;
        for playlist in file.playlists {
            match self
                .playlists
                .items
                .iter()
                .position(|p| p.name == playlist.name)
            {
                Some(index) => {
                    for track in playlist.tracks {
                        if self.playlists.add_track(index, track) {
                            tracks_new += 1;
                        }
                    }
                }
                None => {
                    playlists_new += 1;
                    self.playlists.items.push(playlist);
                }
            }
        }
        if favs > 0 {
            self.save_favorites();
        }
        if playlists_new > 0 || tracks_new > 0 {
            self.save_playlists();
        }
        Ok(format!(
            "importado: {favs} favoritos, {playlists_new} playlists nuevas, {tracks_new} pistas añadidas"
        ))
    }

    fn import_m3u_text(&mut self, path: &std::path::Path, text: &str) -> Result<String, String> {
        let tracks = crate::portable::import_m3u(text)?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("importada");
        let index = match self
            .playlists
            .items
            .iter()
            .position(|p| p.name == name)
        {
            Some(index) => index,
            None => self.playlists.create(name)?,
        };
        let mut added = 0;
        for track in tracks {
            if self.playlists.add_track(index, track) {
                added += 1;
            }
        }
        self.save_playlists();
        Ok(format!("importado: {added} pistas en «{name}»"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with_history(entries: &[&str]) -> App {
        let mut app = App::with_data_dir(
            &Config::default(),
            &std::env::temp_dir().join(format!("shellmusic-hist-nav-{}", std::process::id())),
        )
        .unwrap();
        for entry in entries {
            app.history.push(entry);
        }
        app
    }

    #[test]
    fn history_navigation_recalls_and_restores_draft() {
        let mut app = app_with_history(&["primera", "segunda", "tercera"]);
        app.input = "borrador".to_string();

        app.history_prev();
        assert_eq!(app.input, "tercera");
        app.history_prev();
        assert_eq!(app.input, "segunda");
        app.history_prev();
        assert_eq!(app.input, "primera");
        // Tope: la más antigua no baja más.
        app.history_prev();
        assert_eq!(app.input, "primera");

        app.history_next();
        assert_eq!(app.input, "segunda");
        app.history_next();
        assert_eq!(app.input, "tercera");
        // Pasar de la más reciente restaura el borrador.
        app.history_next();
        assert_eq!(app.input, "borrador");
    }

    #[test]
    fn history_navigation_with_empty_history_does_nothing() {
        let mut app = app_with_history(&[]);
        app.input = "texto".to_string();
        app.history_prev();
        app.history_next();
        assert_eq!(app.input, "texto");
    }

    #[test]
    fn history_suggestion_completes_from_most_recent() {
        let mut app = app_with_history(&["daft punk", "david bowie", "daft punk remix"]);
        app.input = "da".to_string();
        // La más reciente con ese prefijo gana.
        assert_eq!(app.history_suggestion(), Some("daft punk remix"));
        app.input = "dav".to_string();
        assert_eq!(app.history_suggestion(), Some("david bowie"));
        // Sin prefijo completo no hay sugerencia; input vacío tampoco.
        app.input = "daft punk remix".to_string();
        assert_eq!(app.history_suggestion(), None);
        app.input.clear();
        assert_eq!(app.history_suggestion(), None);
    }

    #[test]
    fn accept_suggestion_completes_input() {
        let mut app = app_with_history(&["daft punk"]);
        app.input = "da".to_string();
        app.accept_suggestion();
        assert_eq!(app.input, "daft punk");
        // Navegando el historial no se sugiere nada.
        let mut app = app_with_history(&["daft punk"]);
        app.input = "otra".to_string();
        app.history_prev();
        assert_eq!(app.history_suggestion(), None);
    }

    #[test]
    fn import_json_merges_without_duplicates() {
        let mut app = App::with_data_dir(
            &Config::default(),
            &std::env::temp_dir().join(format!(
                "shellmusic-import-json-{}-{:016x}",
                std::process::id(),
                crate::state_file::random_suffix()
            )),
        )
        .unwrap();
        let track = Track {
            id: "a".to_string(),
            title: "título a".to_string(),
            artist: "artista".to_string(),
            duration: None,
            source_url: "https://example.com/a".to_string(),
            provider: ProviderKind::Youtube,
        };
        app.favorites.add(track.clone());
        let pl = app.playlists.create("mix").unwrap();
        app.playlists.add_track(pl, track.clone());

        // La copia trae el mismo favorito, la misma pista en «mix» y una
        // playlist nueva: nada se duplica.
        let text = format!(
            "{{\"version\": 1, \"favorites\": [{}], \"playlists\": [{{\"name\": \"mix\", \"tracks\": [{}]}}, {{\"name\": \"nueva\", \"tracks\": [{}]}}]}}",
            serde_json::to_string(&track).unwrap(),
            serde_json::to_string(&track).unwrap(),
            serde_json::to_string(&track).unwrap(),
        );
        let summary = app.import_json_text(&text).unwrap();
        assert!(summary.contains("0 favoritos"), "{summary}");
        assert_eq!(app.favorites.tracks.len(), 1);
        assert_eq!(app.playlists.items.len(), 2);
        let mix = app.playlists.items.iter().find(|p| p.name == "mix").unwrap();
        assert_eq!(mix.tracks.len(), 1, "sin pista duplicada en la merge");
    }

    #[test]
    fn import_m3u_text_creates_playlist() {
        let mut app = App::with_data_dir(
            &Config::default(),
            &std::env::temp_dir().join(format!(
                "shellmusic-import-m3u-{}-{:016x}",
                std::process::id(),
                crate::state_file::random_suffix()
            )),
        )
        .unwrap();
        let text = "#EXTM3U\n#EXTINF:200,artista - canción\nhttps://www.youtube.com/watch?v=abc123\n";
        let path = std::path::Path::new("/tmp/mi mix.m3u");
        let summary = app.import_m3u_text(path, text).unwrap();
        assert!(summary.contains("1 pistas"), "{summary}");
        let pl = app.playlists.items.iter().find(|p| p.name == "mi mix").unwrap();
        assert_eq!(pl.tracks[0].id, "abc123");
        assert_eq!(pl.tracks[0].title, "canción");
    }

    #[test]
    fn select_clampa_mas_alla_del_final() {
        // Base de g/G y PgDn: select(0) va al inicio y select(usize::MAX)
        // clama al final; con lista vacía no selecciona nada.
        let mut app = app_with_history(&[]);
        for i in 0..30 {
            app.results.push(Track {
                id: i.to_string(),
                title: format!("t{i}"),
                artist: "a".to_string(),
                duration: None,
                source_url: String::new(),
                provider: ProviderKind::Youtube,
            });
        }
        app.select(usize::MAX);
        assert_eq!(app.selected(), Some(29));
        app.select(0);
        assert_eq!(app.selected(), Some(0));

        let mut app = app_with_history(&[]);
        app.queue_select(usize::MAX);
        assert_eq!(app.queue_selected(), None);
    }

    #[test]
    fn radio_mode_cycles_through_all() {
        let mut mode = RadioMode::Artista;
        let mut seen = vec![mode];
        for _ in 0..4 {
            mode = mode.next();
            seen.push(mode);
        }
        assert_eq!(mode.next(), RadioMode::Artista, "el ciclo cierra");
        assert_eq!(
            seen,
            [
                RadioMode::Artista,
                RadioMode::Similares,
                RadioMode::Random,
                RadioMode::RandomGenero,
                RadioMode::Rarezas,
            ]
        );
    }

    #[test]
    fn radio_mode_from_name_roundtrip() {
        for name in ["artista", "similares", "random", "random-genero", "rarezas"] {
            assert!(RadioMode::from_name(name).is_some(), "{name}");
        }
        assert_eq!(RadioMode::from_name("inventado"), None);
    }

    #[test]
    fn radio_mode_config_applies() {
        let mut config = Config::default();
        config.radio.mode = "similares".to_string();
        let app = App::with_data_dir(
            &config,
            &std::env::temp_dir().join(format!("shellmusic-radiomode-{}", std::process::id())),
        )
        .unwrap();
        assert_eq!(app.radio_mode, RadioMode::Similares);

        // Modo desconocido → artista (default).
        config.radio.mode = "inventado".to_string();
        let app = App::with_data_dir(
            &config,
            &std::env::temp_dir().join(format!("shellmusic-radiomode2-{}", std::process::id())),
        )
        .unwrap();
        assert_eq!(app.radio_mode, RadioMode::Artista);
    }
}
