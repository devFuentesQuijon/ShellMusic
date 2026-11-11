//! Menú de opciones en la TUI (RF-70, tecla F2): tabla estática de opciones
//! que se aplican en caliente y se persisten en config.toml conservando sus
//! comentarios (ver `Config::update`). Las integraciones notifican a
//! `SystemIntegration` por `AppMsg`, que vive en el run loop.

use tokio::sync::mpsc;

use super::state::{Message, Overlay, RadioMode};
use super::{App, AppMsg};
use crate::config::{Config, UiStyle};

pub const CATEGORIES: &[&str] = &["Apariencia", "Reproducción", "Integraciones"];

/// Pasos de «Resultados por búsqueda» al ciclar con ←/→.
const RESULTS_STEPS: &[usize] = &[5, 10, 15, 20, 30, 50];

const RADIO_MODES: &[RadioMode] = &[
    RadioMode::Artista,
    RadioMode::Similares,
    RadioMode::Random,
    RadioMode::RandomGenero,
    RadioMode::Rarezas,
];

/// Fila del menú: etiqueta fija + valor actual (para dibujar).
pub struct Row {
    pub label: &'static str,
    pub value: String,
}

fn on_off(on: bool) -> String {
    if on { "sí" } else { "no" }.to_string()
}

/// Filas de la categoría dada con su valor actual.
pub fn rows(app: &App, category: usize) -> Vec<Row> {
    match category {
        0 => vec![
            Row { label: "Estilo UI", value: app.ui_style.name().to_string() },
            Row { label: "Tema", value: app.theme_name.clone() },
            Row { label: "Ratón", value: on_off(app.mouse) },
            Row { label: "Resultados por búsqueda", value: app.results_limit.to_string() },
        ],
        1 => vec![
            Row { label: "Gapless (sin pausa)", value: on_off(app.gapless) },
            Row { label: "Radio automática", value: on_off(app.radio) },
            Row { label: "Modo de radio", value: app.radio_mode.to_string() },
        ],
        2 => vec![
            Row { label: "Notificaciones", value: on_off(app.notify_enabled) },
            Row { label: "Presencia en Discord", value: on_off(app.discord_enabled) },
            Row { label: "SponsorBlock", value: on_off(app.sb_enabled) },
            Row { label: "Like write-back YTM", value: on_off(app.ytm_like_writeback) },
        ],
        _ => Vec::new(),
    }
}

/// Abre el menú (F2): carga los temas disponibles y resetea la selección.
pub fn open(app: &mut App) {
    let (themes, warnings) = crate::theme::load_user_themes();
    app.options_themes = crate::theme::preset_names()
        .into_iter()
        .map(str::to_string)
        .chain(themes.into_iter().map(|(name, _)| name))
        .collect();
    app.options_tab = 0;
    app.options_sel.select(Some(0));
    app.overlay = Overlay::Options;
    if let Some(warning) = warnings.first() {
        app.message = Some(Message::error(format!(
            "tema de usuario ignorado: {warning}"
        )));
    }
}

/// Maneja las teclas del overlay (lo enlaza `input.rs`).
pub fn handle_key(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    tx: &mpsc::UnboundedSender<AppMsg>,
) {
    use crossterm::event::{KeyCode, KeyModifiers};
    match key.code {
        KeyCode::Esc | KeyCode::F(2) => app.overlay = Overlay::Closed,
        KeyCode::Tab | KeyCode::BackTab => {
            let dir = if key.code == KeyCode::BackTab
                || key.modifiers.contains(KeyModifiers::SHIFT)
            {
                -1
            } else {
                1
            };
            let len = CATEGORIES.len() as i32;
            app.options_tab = ((app.options_tab as i32 + dir).rem_euclid(len)) as usize;
            app.options_sel.select(Some(0));
        }
        KeyCode::Up => move_sel(app, -1),
        KeyCode::Down => move_sel(app, 1),
        KeyCode::Left | KeyCode::Right | KeyCode::Enter => {
            let category = app.options_tab;
            let Some(row) = app.options_sel.selected() else { return };
            let dir = if key.code == KeyCode::Left { -1 } else { 1 };
            cycle(app, tx, category, row, dir);
        }
        _ => {}
    }
}

fn move_sel(app: &mut App, dir: i32) {
    let len = rows(app, app.options_tab).len();
    if len == 0 {
        return;
    }
    let current = app.options_sel.selected().unwrap_or(0) as i32;
    app.options_sel
        .select(Some((current + dir).rem_euclid(len as i32) as usize));
}

/// Aplica un cambio sobre la fila dada (dir ±1) y lo persiste en config.toml.
pub fn cycle(
    app: &mut App,
    tx: &mpsc::UnboundedSender<AppMsg>,
    category: usize,
    row: usize,
    dir: i32,
) {
    let entries = apply(app, tx, category, row, dir);
    persist(app, entries);
}

/// Aplica el cambio y devuelve las entradas a persistir (vacío si la fila
/// no es editable o el cambio no aplicó).
fn apply(
    app: &mut App,
    tx: &mpsc::UnboundedSender<AppMsg>,
    category: usize,
    row: usize,
    dir: i32,
) -> Vec<(String, toml::Value)> {
    match (category, row) {
        (0, 0) => {
            app.ui_style = step_ui_style(app.ui_style, dir);
            app.message = Some(Message::info(format!(
                "estilo UI «{}» aplicado y guardado",
                app.ui_style.name()
            )));
            vec![("ui.style".to_string(), app.ui_style.name().into())]
        }
        (0, 1) => cycle_theme(app, dir),
        (0, 2) => {
            let on = !app.mouse;
            app.mouse = on;
            // La captura se conmuta aquí (main.rs la activa al arrancar y la
            // libera al salir); al apagar, los eventos dejan de llegar.
            if on {
                let _ = crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture);
            } else {
                let _ =
                    crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
            }
            vec![("general.mouse".to_string(), on.into())]
        }
        (0, 3) => {
            let idx = RESULTS_STEPS
                .iter()
                .position(|step| *step >= app.results_limit)
                .unwrap_or(2);
            let next = RESULTS_STEPS
                .get((idx as i32 + dir).rem_euclid(RESULTS_STEPS.len() as i32) as usize)
                .copied()
                .unwrap_or(15);
            app.results_limit = next;
            vec![("general.results_per_search".to_string(), (next as i64).into())]
        }
        (1, 0) => {
            app.gapless = !app.gapless;
            vec![("audio.gapless".to_string(), app.gapless.into())]
        }
        (1, 1) => {
            app.radio = !app.radio;
            vec![("radio.enabled".to_string(), app.radio.into())]
        }
        (1, 2) => {
            let idx = RADIO_MODES
                .iter()
                .position(|mode| *mode == app.radio_mode)
                .unwrap_or(0);
            let next = RADIO_MODES
                [(idx as i32 + dir).rem_euclid(RADIO_MODES.len() as i32) as usize];
            app.radio_mode = next;
            vec![("radio.mode".to_string(), next.name().into())]
        }
        (2, 0) => {
            let on = !app.notify_enabled;
            app.notify_enabled = on;
            let _ = tx.send(AppMsg::SetNotifications(on));
            vec![("integrations.notifications".to_string(), on.into())]
        }
        (2, 1) => {
            let on = !app.discord_enabled;
            app.discord_enabled = on;
            let _ = tx.send(AppMsg::SetDiscord(on));
            vec![("integrations.discord".to_string(), on.into())]
        }
        (2, 2) => {
            app.sb_enabled = !app.sb_enabled;
            vec![("sponsorblock.enabled".to_string(), app.sb_enabled.into())]
        }
        (2, 3) => {
            app.ytm_like_writeback = !app.ytm_like_writeback;
            vec![("ytm.like_writeback".to_string(), app.ytm_like_writeback.into())]
        }
        _ => Vec::new(),
    }
}

fn step_ui_style(current: UiStyle, dir: i32) -> UiStyle {
    match (current, dir.is_negative()) {
        (UiStyle::Classic, false) | (UiStyle::Modern, true) => UiStyle::Modern,
        _ => UiStyle::Classic,
    }
}

/// Cicla el tema por la lista cargada al abrir el menú (presets + usuario).
fn cycle_theme(app: &mut App, dir: i32) -> Vec<(String, toml::Value)> {
    if app.options_themes.is_empty() {
        return Vec::new();
    }
    let len = app.options_themes.len() as i32;
    let idx = app
        .options_themes
        .iter()
        .position(|name| *name == app.theme_name)
        .map_or(0, |i| i as i32);
    let next = app.options_themes[((idx + dir).rem_euclid(len)) as usize].clone();
    apply_theme(app, &next)
}

/// Aplica un tema (preset o de usuario) en caliente y devuelve las entradas
/// para persistir sus colores en `[theme]`. Lo comparten el menú y `: theme`.
pub fn apply_theme(app: &mut App, name: &str) -> Vec<(String, toml::Value)> {
    match crate::theme::find_theme(name) {
        Some((config, canonical)) => {
            let (theme, warning) = crate::theme::Theme::resolve(&config);
            app.theme = theme;
            app.theme_name = canonical.clone();
            app.message = Some(match warning {
                Some(warning) => Message::error(format!("tema «{canonical}»: {warning}")),
                None => Message::info(format!("tema «{canonical}» aplicado y guardado")),
            });
            vec![
                ("theme.accent".to_string(), config.accent.into()),
                ("theme.highlight".to_string(), config.highlight.into()),
                ("theme.error".to_string(), config.error.into()),
                ("theme.muted".to_string(), config.muted.into()),
                ("theme.progress".to_string(), config.progress.into()),
            ]
        }
        None => {
            app.message = Some(Message::error(format!(
                "tema desconocido: «{name}». Disponibles: {}",
                crate::theme::all_theme_names().join(", ")
            )));
            Vec::new()
        }
    }
}

/// Fija el estilo UI (`: ui`, menú) y lo persiste.
pub fn set_style(app: &mut App, style: UiStyle) {
    app.ui_style = style;
    app.message = Some(Message::info(format!(
        "estilo UI «{}» aplicado y guardado",
        style.name()
    )));
    persist(app, vec![("ui.style".to_string(), style.name().into())]);
}

/// Aplica un tema y lo persiste (lo comparten el menú y `: theme`).
pub fn apply_theme_and_save(app: &mut App, name: &str) {
    let entries = apply_theme(app, name);
    persist(app, entries);
}

fn persist(app: &mut App, entries: Vec<(String, toml::Value)>) {
    if entries.is_empty() {
        return;
    }
    let path = app.config_path.clone();
    if let Err(err) = Config::update_at(&path, &entries) {
        app.message = Some(Message::error(format!(
            "no se pudo guardar la configuración: {err:#}"
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Config mínima con las claves que el menú toca (los tests escriben el
    /// archivo real en un dir temporal vía `app.config_path`). Los valores
    /// iniciales reflejan `Config::default()`: notifications = true.
    const SAMPLE_CONFIG: &str = "\
[general]
results_per_search = 15
mouse = true

[ui]
style = \"classic\"

[radio]
enabled = false
mode = \"artista\"

[theme]
accent = \"cyan\"
highlight = \"yellow\"
error = \"red\"
muted = \"darkgray\"
progress = \"green\"

[integrations]
notifications = true
discord = false

[sponsorblock]
enabled = false

[ytm]
like_writeback = true

[audio]
gapless = false
";

    fn app_with_temp_config() -> (App, std::path::PathBuf) {
        let data_dir = std::env::temp_dir().join(format!(
            "shellmusic-options-{}-{:016x}",
            std::process::id(),
            crate::state_file::random_suffix()
        ));
        std::fs::create_dir_all(&data_dir).unwrap();
        let config_path = data_dir.join("config.toml");
        std::fs::write(&config_path, SAMPLE_CONFIG).unwrap();
        let mut app = App::with_data_dir(&Config::default(), &data_dir).unwrap();
        app.config_path = config_path.clone();
        (app, config_path)
    }

    fn cleanup(app: &App) {
        let _ = std::fs::remove_dir_all(&app.data_dir);
    }

    #[test]
    fn cycle_style_flips_and_persists() {
        let (mut app, config_path) = app_with_temp_config();
        assert_eq!(app.ui_style, UiStyle::Classic);
        let (tx, _rx) = mpsc::unbounded_channel();
        cycle(&mut app, &tx, 0, 0, 1);
        assert_eq!(app.ui_style, UiStyle::Modern);
        let text = std::fs::read_to_string(&config_path).unwrap();
        assert!(text.contains("style = \"modern\""), "config:\n{text}");
        cycle(&mut app, &tx, 0, 0, 1);
        assert_eq!(app.ui_style, UiStyle::Classic, "cicla de vuelta");
        cleanup(&app);
    }

    #[test]
    fn cycle_radio_mode_steps_and_wraps() {
        let (mut app, config_path) = app_with_temp_config();
        let (tx, _rx) = mpsc::unbounded_channel();
        assert_eq!(app.radio_mode, RadioMode::Artista);
        cycle(&mut app, &tx, 1, 2, 1);
        assert_eq!(app.radio_mode, RadioMode::Similares);
        cycle(&mut app, &tx, 1, 2, -1);
        cycle(&mut app, &tx, 1, 2, -1);
        assert_eq!(app.radio_mode, RadioMode::Rarezas, "envuelve hacia atrás");
        let text = std::fs::read_to_string(&config_path).unwrap();
        assert!(text.contains("mode = \"rarezas\""), "config:\n{text}");
        cleanup(&app);
    }

    #[test]
    fn cycle_results_steps_and_wraps_at_edges() {
        let (mut app, _) = app_with_temp_config();
        let (tx, _rx) = mpsc::unbounded_channel();
        assert_eq!(app.results_limit, 15);
        cycle(&mut app, &tx, 0, 3, -1);
        assert_eq!(app.results_limit, 10);
        cycle(&mut app, &tx, 0, 3, 1);
        assert_eq!(app.results_limit, 15);
        // En el mínimo, hacia atrás envuelve al máximo; y viceversa.
        app.results_limit = 5;
        cycle(&mut app, &tx, 0, 3, -1);
        assert_eq!(app.results_limit, 50);
        cycle(&mut app, &tx, 0, 3, 1);
        assert_eq!(app.results_limit, 5);
        cleanup(&app);
    }

    #[test]
    fn cycle_integration_sends_system_msg() {
        let (mut app, config_path) = app_with_temp_config();
        let (tx, mut rx) = mpsc::unbounded_channel();
        // Config::default() trae notificaciones activas: el toggle apaga.
        cycle(&mut app, &tx, 2, 0, 1);
        assert!(matches!(rx.try_recv(), Ok(AppMsg::SetNotifications(false))));
        assert!(!app.notify_enabled);
        cycle(&mut app, &tx, 2, 1, 1);
        assert!(matches!(rx.try_recv(), Ok(AppMsg::SetDiscord(true))));
        let text = std::fs::read_to_string(&config_path).unwrap();
        assert!(text.contains("notifications = false"), "config:\n{text}");
        assert!(text.contains("discord = true"), "config:\n{text}");
        cleanup(&app);
    }

    #[test]
    fn cycle_theme_applies_and_persists_colors() {
        let (mut app, config_path) = app_with_temp_config();
        let (tx, _rx) = mpsc::unbounded_channel();
        open(&mut app);
        // «personalizado» no está en la lista: el primer ciclo va al segundo.
        cycle(&mut app, &tx, 0, 1, 1);
        assert_eq!(app.theme_name, "frankenstein");
        let text = std::fs::read_to_string(&config_path).unwrap();
        assert!(text.contains("accent = \"#00e676\""), "config:\n{text}");
        cycle(&mut app, &tx, 0, 1, -1);
        cycle(&mut app, &tx, 0, 1, -1);
        assert_eq!(app.theme_name, "thriller", "envuelve hacia atrás");
        cleanup(&app);
    }
}
