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
    pub providers: Vec<(ProviderKind, Arc<dyn MusicProvider>)>,
    pub active_provider: usize,
    pub radio: bool,
    pub played: HashSet<(ProviderKind, String)>,
    /// Títulos normalizados ya reproducidos: la radio no encola otra versión
    /// de la misma canción.
    pub played_titles: HashSet<String>,
    pub radio_epoch: u64,
    pub(super) results_limit: usize,
    favorites_path: std::path::PathBuf,
    playlists_path: std::path::PathBuf,
    pub history: History,
    /// Posición al navegar el historial con ↑/↓; None = editando texto nuevo.
    history_index: Option<usize>,
    /// Borrador del input al empezar a navegar el historial; se restaura al
    /// bajar más allá de la búsqueda más reciente.
    history_draft: String,
    history_path: std::path::PathBuf,
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
        let (favorites, fav_warning) = Favorites::load(&favorites_path)
            .with_context(|| format!("no se pudieron leer {}", favorites_path.display()))?;
        let (playlists, pl_warning) = Playlists::load(&playlists_path)
            .with_context(|| format!("no se pudieron leer {}", playlists_path.display()))?;
        let (history, hist_warning) = History::load(&history_path)
            .with_context(|| format!("no se pudo leer {}", history_path.display()))?;
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
            message: fav_warning.or(pl_warning).or(hist_warning).map(Message::error),
            searching: false,
            should_quit: false,
            providers: vec![(ProviderKind::Youtube, Arc::new(YoutubeProvider::new()))],
            active_provider: 0,
            radio: config.radio.enabled,
            played: HashSet::new(),
            played_titles: HashSet::new(),
            radio_epoch: 0,
            results_limit: config.general.results_per_search,
            favorites_path,
            playlists_path,
            history,
            history_index: None,
            history_draft: String::new(),
            history_path,
        };
        if !app.favorites.tracks.is_empty() {
            app.favorites_state.select(Some(0));
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
}
