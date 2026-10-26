use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::providers::ProviderKind;

const DEFAULT_TOML: &str = "\
[general]
results_per_search = 15   # 1-50
providers = [\"youtube\", \"soundcloud\"]   # proveedores habilitados; la tecla m cicla entre ellos
default_provider = \"youtube\"              # proveedor activo al arrancar (debe estar en la lista)

[audio]
initial_volume = 70       # 0-100
format = \"\"               # selector de formato que recibe yt-dlp (mpv --ytdl-format): \"bestaudio\", \"worst\", \"bestaudio[abr<=128]\", ...; \"\" = criterio por defecto de yt-dlp

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
";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub audio: Audio,
    pub youtube: Youtube,
    pub cache: Cache,
    pub radio: Radio,
    pub theme: ThemeConfig,
    pub integrations: Integrations,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct General {
    pub results_per_search: usize,
    pub providers: Vec<String>,
    pub default_provider: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Audio {
    pub initial_volume: u8,
    /// Selector de formato de extracción de yt-dlp (mpv `--ytdl-format`);
    /// vacío = criterio por defecto de yt-dlp.
    pub format: String,
}

impl Default for General {
    fn default() -> Self {
        Self {
            results_per_search: 15,
            providers: vec!["youtube".to_string(), "soundcloud".to_string()],
            default_provider: "youtube".to_string(),
        }
    }
}

impl Default for Audio {
    fn default() -> Self {
        Self {
            initial_volume: 70,
            format: String::new(),
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
}

impl Default for Integrations {
    fn default() -> Self {
        Self {
            notifications: true,
            media_controls: true,
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
        Ok(config)
    }
}
