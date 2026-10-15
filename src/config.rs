use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

const DEFAULT_TOML: &str = "\
[general]
results_per_search = 15   # 1-50

[audio]
initial_volume = 70       # 0-100

[youtube]
cookies_from_browser = \"auto\"   # \"auto\": primero sin cookies y, si YouTube bloquea, reintenta con cada navegador instalado; \"firefox\"/\"chrome\"/...: solo ese; \"\": nunca usar cookies
player_client = \"android\"          # cliente de YouTube que yt-dlp emula (\"android\" evita el 403 por PO Token; \"\" = cliente por defecto)
remote_components = \"ejs:github\"   # permite a yt-dlp resolver los retos JS de firma (\"\" = desactivado)

[visualizer]
enabled = false   # espectro animado (solo Linux; captura el audio del sistema)
bands = 24        # 8-64
fps = 20          # 5-60
";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub audio: Audio,
    pub youtube: Youtube,
    pub visualizer: Visualizer,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct General {
    pub results_per_search: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Audio {
    pub initial_volume: u8,
}

impl Default for General {
    fn default() -> Self {
        Self {
            results_per_search: 15,
        }
    }
}

impl Default for Audio {
    fn default() -> Self {
        Self { initial_volume: 70 }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Youtube {
    pub cookies_from_browser: String,
    pub player_client: String,
    pub remote_components: String,
}

impl Default for Youtube {
    fn default() -> Self {
        Self {
            cookies_from_browser: "auto".to_string(),
            player_client: "android".to_string(),
            remote_components: "ejs:github".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Visualizer {
    pub enabled: bool,
    pub bands: usize,
    pub fps: u16,
}

impl Default for Visualizer {
    fn default() -> Self {
        Self {
            enabled: false,
            bands: 24,
            fps: 20,
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
        config.audio.initial_volume = config.audio.initial_volume.min(100);
        config.youtube.cookies_from_browser = config.youtube.cookies_from_browser.trim().to_string();
        config.youtube.player_client = config.youtube.player_client.trim().to_string();
        config.youtube.remote_components = config.youtube.remote_components.trim().to_string();
        config.visualizer.bands = config.visualizer.bands.clamp(8, 64);
        config.visualizer.fps = config.visualizer.fps.clamp(5, 60);
        Ok(config)
    }
}
