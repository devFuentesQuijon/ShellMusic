use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::providers::ProviderKind;

const DEFAULT_TOML: &str = "\
[general]
results_per_search = 15   # 1-50
providers = [\"youtube\", \"soundcloud\", \"bandcamp\"]   # proveedores habilitados; la tecla m cicla entre ellos
default_provider = \"youtube\"              # proveedor activo al arrancar (debe estar en la lista)
mouse = true                # ratón: click en la barra = seek · rueda = volumen · click en panel = foco (false deja la selección de texto del terminal)

[ui]
style = \"classic\"    # classic | modern — modern: bordes redondeados y barra de progreso con degradado

[audio]
initial_volume = 70       # 0-100
format = \"\"               # selector de formato que recibe yt-dlp (mpv --ytdl-format): \"bestaudio\", \"worst\", \"bestaudio[abr<=128]\", ...; \"\" = criterio por defecto de yt-dlp
gapless = false           # true: mpv apila la siguiente pista de la cola y la enchufa sin pausa (--gapless-audio); extrae la URL de la siguiente por adelantado
crossfade = 0             # 0 = off · 1-10: fundido de salida en los últimos N s y de entrada en los primeros N s de cada pista (afade); combínalo con gapless

[youtube]
cookies_from_browser = \"auto\"   # \"auto\": primero sin cookies y, si YouTube bloquea, reintenta con cada navegador instalado; \"firefox\"/\"chrome\"/...: solo ese; \"\": nunca usar cookies
player_client = \"android\"          # cliente de YouTube que yt-dlp emula primero (\"android\" evita el 403 por PO Token; \"\" = cliente por defecto). Si bloquea, se rota por ios/tv/web_embedded
remote_components = \"ejs:github\"   # permite a yt-dlp resolver los retos JS de firma (\"\" = desactivado)
pot_provider = \"auto\"              # \"auto\": usar el plugin bgutil PO Token de yt-dlp si está instalado (\"off\" = ignorarlo)

[cache]
enabled = true      # caché en memoria de búsquedas (solo dura la sesión)
ttl_minutes = 15    # 1-120

[radio]
enabled = false     # al agotarse la cola, encola pistas relacionadas (se alterna con la tecla r)
mode = \"artista\"    # artista | similares | random | random-genero | rarezas (se cicla con R)

[theme]
accent = \"cyan\"        # bordes del panel enfocado y títulos de sección
highlight = \"yellow\"   # mensajes informativos y teclas en la ayuda
error = \"red\"          # mensajes de error
muted = \"darkgray\"     # línea de hints y texto secundario
progress = \"green\"     # barra de progreso

[integrations]
notifications = true    # notificación de escritorio al cambiar de pista
media_controls = true   # MPRIS en Linux: teclas multimedia, widgets, KDE Connect
discord = false         # presencia en Discord («escuchando: …»); requiere el cliente Discord abierto
discord_client_id = \"\"   # override para forks; vacío = el ID embebido en el binario

[lastfm]
enabled = false         # scrobbling de escucha a Last.fm
api_key = \"\"             # créalas en https://www.last.fm/api/account/create
secret = \"\"              # y autoriza con: smusic --lastfm-auth

[mcp]
enabled = false         # socket de control local para agentes de IA (cliente de referencia: smusic-mcp)

[sponsorblock]
enabled = false         # saltar segmentos (sponsor/intro/outro) via sponsor.ajay.app; AVISO: envía el ID de cada vídeo reproducido a ese servidor
categories = [\"sponsor\", \"intro\", \"outro\"]   # también: interaction, selfpromo, preview, music_offtopic, filler

[ytm]
enabled = false         # YouTube Music con tu cuenta (pip install ytmusicapi y alta con: smusic --ytm-auth)
like_writeback = true   # la tecla f también da/quita like en YouTube Music
";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub ui: UiConfig,
    pub audio: Audio,
    pub youtube: Youtube,
    pub cache: Cache,
    pub radio: Radio,
    pub theme: ThemeConfig,
    pub integrations: Integrations,
    pub lastfm: Lastfm,
    pub mcp: Mcp,
    pub sponsorblock: Sponsorblock,
    pub ytm: Ytm,
}

/// Estilo de interfaz (`[ui]`, RF-69): el «moderno» es el chrome inspirado en
/// btop (bordes redondeados y barra con degradado); el «clásico» es el look
/// original. Cambiable en caliente con `: ui` o el menú F2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiStyle {
    /// Look original: bordes cuadrados y barra de progreso plana.
    #[default]
    Classic,
    /// Chrome inspirado en btop: bordes redondeados y degradados.
    Modern,
}

impl UiStyle {
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_lowercase().as_str() {
            "classic" | "clasico" | "clásico" => Some(Self::Classic),
            "modern" | "moderno" => Some(Self::Modern),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Modern => "modern",
        }
    }

    /// El otro estilo (para `: ui` sin argumento, que alterna).
    pub fn toggled(self) -> Self {
        match self {
            Self::Classic => Self::Modern,
            Self::Modern => Self::Classic,
        }
    }
}

/// Sección `[ui]`: apariencia de la TUI.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// "classic" | "modern" (se valida en `Config::load`).
    pub style: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            style: "classic".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct General {
    pub results_per_search: usize,
    pub providers: Vec<String>,
    pub default_provider: String,
    /// Captura de ratón (click en la barra = seek, rueda = volumen, click
    /// en panel = foco). Apagado, la selección de texto del terminal sigue
    /// disponible.
    pub mouse: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Audio {
    pub initial_volume: u8,
    /// Selector de formato de extracción de yt-dlp (mpv `--ytdl-format`);
    /// vacío = criterio por defecto de yt-dlp.
    pub format: String,
    /// Gapless: mpv apila la siguiente pista de la cola mientras suena la
    /// actual y la enchufa sin pausa al terminar (`--gapless-audio`).
    /// A cambio, extrae la URL de la siguiente pista por adelantado.
    pub gapless: bool,
    /// Crossfade percibido (segundos; 0 = desactivado): fundido de salida en
    /// los últimos N segundos de cada pista y de entrada en los primeros N.
    pub crossfade: u32,
}

impl Default for General {
    fn default() -> Self {
        Self {
            results_per_search: 15,
            providers: vec![
                "youtube".to_string(),
                "soundcloud".to_string(),
                "bandcamp".to_string(),
            ],
            default_provider: "youtube".to_string(),
            mouse: true,
        }
    }
}

impl Default for Audio {
    fn default() -> Self {
        Self {
            initial_volume: 70,
            format: String::new(),
            gapless: false,
            crossfade: 0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Youtube {
    pub cookies_from_browser: String,
    pub player_client: String,
    pub remote_components: String,
    /// "auto": usar el plugin bgutil PO Token si está instalado; "off": ignorarlo.
    pub pot_provider: String,
}

impl Default for Youtube {
    fn default() -> Self {
        Self {
            cookies_from_browser: "auto".to_string(),
            player_client: "android".to_string(),
            remote_components: "ejs:github".to_string(),
            pot_provider: "auto".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Cache {
    pub enabled: bool,
    pub ttl_minutes: u64,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl_minutes: 15,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Radio {
    pub enabled: bool,
    /// artista | similares | random | random-genero | rarezas
    pub mode: String,
}

/// Colores de la TUI (sección `[theme]`); se resuelven a `crate::theme::Theme`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    /// Bordes del panel enfocado y títulos de sección.
    pub accent: String,
    /// Mensajes informativos y teclas en la ayuda.
    pub highlight: String,
    /// Mensajes de error.
    pub error: String,
    /// Línea de hints y texto secundario.
    pub muted: String,
    /// Barra de progreso del reproductor.
    pub progress: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            accent: "cyan".to_string(),
            highlight: "yellow".to_string(),
            error: "red".to_string(),
            muted: "darkgray".to_string(),
            progress: "green".to_string(),
        }
    }
}

/// Integración con el sistema operativo (sección `[integrations]`).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Integrations {
    /// Notificación de escritorio al cambiar de pista.
    pub notifications: bool,
    /// Controles multimedia del SO (MPRIS; solo tiene efecto en Linux).
    pub media_controls: bool,
    /// Presencia en Discord («escuchando: …») vía IPC local. Requiere el
    /// cliente de Discord abierto.
    pub discord: bool,
    /// Override del Application ID para forks; vacío = el embebido en el
    /// binario (los usuarios no necesitan crear nada).
    pub discord_client_id: String,
}

impl Default for Integrations {
    fn default() -> Self {
        Self {
            notifications: true,
            media_controls: true,
            discord: false,
            discord_client_id: String::new(),
        }
    }
}

/// Scrobbling de Last.fm (`[lastfm]`). La sesión autorizada se guarda en
/// `lastfm.json` en el directorio de datos (vía `smusic --lastfm-auth`);
/// estos tres campos viven en config.toml.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Lastfm {
    pub enabled: bool,
    pub api_key: String,
    pub secret: String,
}

/// Socket de control para agentes de IA (`[mcp]`, RF-63). Opt-in: el socket
/// solo existe con `enabled = true` y la app en ejecución.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Mcp {
    pub enabled: bool,
}

/// Salto automático de segmentos con SponsorBlock (RF-67). Opt-in: envía el
/// ID de cada vídeo reproducido a sponsor.ajay.app (documentado en el TOML).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Sponsorblock {
    pub enabled: bool,
    /// Categorías a saltar: sponsor, intro, outro, interaction, selfpromo,
    /// preview, music_offtopic, filler.
    pub categories: Vec<String>,
}

impl Default for Sponsorblock {
    fn default() -> Self {
        Self {
            enabled: false,
            categories: vec![
                "sponsor".to_string(),
                "intro".to_string(),
                "outro".to_string(),
            ],
        }
    }
}

/// YouTube Music con cuenta de Google (`[ytm]`, RF-68), vía el puente Python
/// ytmusicapi (opt-in; alta interactiva con `smusic --ytm-auth`).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Ytm {
    pub enabled: bool,
    /// La tecla `f` (favoritos) también da/quita like en YouTube Music.
    pub like_writeback: bool,
}

impl Default for Ytm {
    fn default() -> Self {
        Self {
            enabled: false,
            like_writeback: true,
        }
    }
}

impl Config {
    pub fn path() -> Result<PathBuf> {
        let base = dirs::config_dir()
            .ok_or_else(|| anyhow!("no se pudo determinar el directorio de configuración"))?;
        Ok(base.join("shellmusic").join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)
                    .with_context(|| format!("no se pudo crear {}", dir.display()))?;
            }
            fs::write(&path, DEFAULT_TOML)
                .with_context(|| format!("no se pudo escribir {}", path.display()))?;
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("no se pudo leer {}", path.display()))?;
        let mut config: Config =
            toml::from_str(&text).with_context(|| format!("{} inválido", path.display()))?;
        config.general.results_per_search = config.general.results_per_search.clamp(1, 50);
        config
            .general
            .providers
            .retain(|name| ProviderKind::from_name(name).is_some());
        if config.general.providers.is_empty() {
            config.general.providers.push("youtube".to_string());
        }
        if !config.general.providers.contains(&config.general.default_provider) {
            config.general.default_provider = config.general.providers[0].clone();
        }
        config.audio.initial_volume = config.audio.initial_volume.min(100);
        config.audio.format = config.audio.format.trim().to_string();
        config.youtube.cookies_from_browser = config.youtube.cookies_from_browser.trim().to_string();
        config.youtube.player_client = config.youtube.player_client.trim().to_string();
        config.youtube.remote_components = config.youtube.remote_components.trim().to_string();
        config.youtube.pot_provider = config.youtube.pot_provider.trim().to_string();
        config.cache.ttl_minutes = config.cache.ttl_minutes.clamp(1, 120);
        config
            .sponsorblock
            .categories
            .retain(|category| !category.trim().is_empty());
        Ok(config)
    }

    /// Persiste cambios sueltos conservando los comentarios del archivo (la
    /// serialización TOML completa los perdería). Cada entrada es
    /// "seccion.clave" con un valor escalar; si la clave o la sección no
    /// existen, se añaden (RF-70, menú de opciones y `: ui`/`: theme`).
    pub fn update(entries: &[(String, toml::Value)]) -> Result<()> {
        Self::update_at(&Self::path()?, entries)
    }

    /// Versión con ruta explícita (tests; ver `update`).
    pub fn update_at(path: &Path, entries: &[(String, toml::Value)]) -> Result<()> {
        let text = if path.exists() {
            fs::read_to_string(path).with_context(|| format!("no se pudo leer {}", path.display()))?
        } else {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)
                    .with_context(|| format!("no se pudo crear {}", dir.display()))?;
            }
            fs::write(path, DEFAULT_TOML)
                .with_context(|| format!("no se pudo escribir {}", path.display()))?;
            DEFAULT_TOML.to_string()
        };
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        for (entry, value) in entries {
            let (section, key) = entry
                .split_once('.')
                .ok_or_else(|| anyhow!("clave de config sin sección: {entry:?}"))?;
            set_line(&mut lines, section.trim(), key.trim(), &value.to_string());
        }
        fs::write(path, lines.join("\n") + "\n")
            .with_context(|| format!("no se pudo escribir {}", path.display()))
    }
}

/// Sustituye (o inserta) `key = literal` en la sección dada de `lines`,
/// conservando el resto del archivo tal cual.
fn set_line(lines: &mut Vec<String>, section: &str, key: &str, literal: &str) {
    let mut current = String::new();
    let mut section_end: Option<usize> = None;
    let mut hit: Option<(usize, String)> = None;
    for (i, raw) in lines.iter().enumerate() {
        let trimmed = raw.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current = trimmed.trim_matches(|c| c == '[' || c == ']').trim().to_string();
            if current == section {
                section_end = Some(i + 1);
            }
            continue;
        }
        if current == section && hit.is_none() && raw.contains('=') {
            let (raw_key, _) = raw.split_once('=').expect("contain '='");
            if raw_key.trim() == key {
                hit = Some((i, replace_value(raw, literal)));
            } else {
                section_end = Some(i + 1);
            }
        }
    }
    if let Some((index, line)) = hit {
        lines[index] = line;
        return;
    }
    let line = format!("{key} = {literal}");
    match section_end {
        Some(at) => lines.insert(at, line),
        None => {
            lines.push(format!("[{section}]"));
            lines.push(line);
        }
    }
}

/// Sustituye el valor de «clave = valor   # comentario» conservando la
/// clave, la indentación, el hueco y el comentario final. El comentario es
/// el último «#…» de la línea; los valores que se editan por menú (bool,
/// entero, enum) nunca contienen «#» en su representación.
fn replace_value(line: &str, literal: &str) -> String {
    let Some((before, after)) = line.split_once('=') else {
        return line.to_string();
    };
    let comment = after
        .rfind('#')
        .map(|at| {
            let head = &after[..at];
            let gap = head.len() - head.trim_end().len();
            &after[head.len() - gap..]
        })
        .unwrap_or("");
    format!("{before}= {literal}{comment}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "shellmusic-config-{}-{}-{:016x}.toml",
            tag,
            std::process::id(),
            crate::state_file::random_suffix()
        ))
    }

    const SAMPLE: &str = "\
[general]
results_per_search = 15   # 1-50
mouse = true                # ratón: click en la barra = seek

[ui]
style = \"classic\"    # classic | modern

[theme]
accent = \"cyan\"        # bordes del panel enfocado
";

    #[test]
    fn ui_style_from_name_accepts_aliases() {
        assert_eq!(UiStyle::from_name("classic"), Some(UiStyle::Classic));
        assert_eq!(UiStyle::from_name("CLÁSICO"), Some(UiStyle::Classic));
        assert_eq!(UiStyle::from_name("modern"), Some(UiStyle::Modern));
        assert_eq!(UiStyle::from_name(" Moderno "), Some(UiStyle::Modern));
        assert_eq!(UiStyle::from_name("raro"), None);
        assert_eq!(UiStyle::Classic.toggled(), UiStyle::Modern);
        assert_eq!(UiStyle::Modern.toggled(), UiStyle::Classic);
        assert_eq!(UiStyle::Modern.name(), "modern");
    }

    #[test]
    fn update_replaces_value_and_keeps_comment() {
        let path = temp_path("replace");
        fs::write(&path, SAMPLE).unwrap();
        Config::update_at(
            &path,
            &[("ui.style".to_string(), "modern".into())],
        )
        .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("style = \"modern\"    # classic | modern"),
            "se conserva el comentario:\n{text}"
        );
        let config: Config = toml::from_str(&text).unwrap();
        assert_eq!(config.ui.style, "modern");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn update_bool_and_number_roundtrip() {
        let path = temp_path("scalars");
        fs::write(&path, SAMPLE).unwrap();
        Config::update_at(
            &path,
            &[
                ("general.mouse".to_string(), false.into()),
                ("general.results_per_search".to_string(), 20.into()),
            ],
        )
        .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("mouse = false"), "texto:\n{text}");
        assert!(text.contains("results_per_search = 20   # 1-50"), "texto:\n{text}");
        let config: Config = toml::from_str(&text).unwrap();
        assert!(!config.general.mouse);
        assert_eq!(config.general.results_per_search, 20);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn update_inserts_missing_key_inside_section() {
        let path = temp_path("insert-key");
        let sample = "[theme]\naccent = \"cyan\"\n\n[other]\nx = 1\n";
        fs::write(&path, sample).unwrap();
        Config::update_at(
            &path,
            &[("theme.progress".to_string(), "#00ff00".into())],
        )
        .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let theme = text.split("[other]").next().unwrap();
        assert!(theme.contains("accent = \"cyan\""), "tema:\n{theme}");
        assert!(
            theme.contains("progress = \"#00ff00\""),
            "la clave nueva va en su sección:\n{theme}"
        );
        fs::remove_file(&path).ok();
    }

    #[test]
    fn update_appends_missing_section_at_end() {
        let path = temp_path("insert-section");
        fs::write(&path, SAMPLE).unwrap();
        Config::update_at(
            &path,
            &[("audio.gapless".to_string(), true.into())],
        )
        .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let pos_section = text.find("[audio]").expect("sección añadida");
        let pos_key = text.find("gapless = true").expect("clave añadida");
        assert!(pos_key > pos_section);
        let config: Config = toml::from_str(&text).unwrap();
        assert!(config.audio.gapless);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn update_audio_crossfade_preserves_comment() {
        let path = temp_path("update-crossfade");
        Config::update_at(&path, &[("audio.crossfade".to_string(), 5.into())]).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let line = text.lines().find(|l| l.contains("crossfade = 5")).expect("clave");
        assert!(line.contains('#'), "el comentario documentado se conserva: {line}");
        let config: Config = toml::from_str(&text).unwrap();
        assert_eq!(config.audio.crossfade, 5);
        // El default sigue siendo off.
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.audio.crossfade, 0);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn update_creates_file_from_default_template() {
        let path = temp_path("create");
        Config::update_at(&path, &[("ui.style".to_string(), "modern".into())]).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("[general]"), "plantilla por defecto");
        assert!(text.contains("style = \"modern\""));
        let config: Config = toml::from_str(&text).unwrap();
        assert_eq!(config.ui.style, "modern");
        assert_eq!(config.general.results_per_search, 15);
        fs::remove_file(&path).ok();
    }
}
