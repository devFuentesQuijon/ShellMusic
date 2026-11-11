//! Tema de color de la TUI, configurable en la sección `[theme]` de
//! `config.toml`. Los valores se parsean con `Color::from_str` de ratatui:
//! nombres ("cyan", "lightred", "darkgray"), hex ("#rrggbb") e índices 0-255.

use std::str::FromStr;

use ratatui::style::Color;

use crate::config::ThemeConfig;

/// Colores resueltos que usa toda la TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Bordes del panel enfocado y títulos de sección.
    pub accent: Color,
    /// Mensajes informativos y teclas en la ayuda.
    pub highlight: Color,
    /// Mensajes de error.
    pub error: Color,
    /// Línea de hints y texto secundario.
    pub muted: Color,
    /// Barra de progreso del reproductor.
    pub progress: Color,
}

/// Un preset con nombre, en crudo (campos &'static str porque const no
/// admite to_string). Fuente única de verdad de los temas empaquetados;
/// docs/temas.md es la vista legible de esta lista.
#[derive(Debug, Clone, Copy)]
pub struct Preset {
    pub accent: &'static str,
    pub highlight: &'static str,
    pub error: &'static str,
    pub muted: &'static str,
    pub progress: &'static str,
}

impl Preset {
    fn into_config(self) -> ThemeConfig {
        ThemeConfig {
            accent: self.accent.to_string(),
            highlight: self.highlight.to_string(),
            error: self.error.to_string(),
            muted: self.muted.to_string(),
            progress: self.progress.to_string(),
        }
    }
}

pub const PRESETS: &[(&str, Preset)] = &[
    (
        "dracula",
        Preset {
            accent: "#bd93f9",
            highlight: "#ff79c6",
            error: "#ff5555",
            muted: "#6272a4",
            progress: "#8be9fd",
        },
    ),
    (
        "frankenstein",
        Preset {
            accent: "#00e676",
            highlight: "#76ff03",
            error: "#ff5252",
            muted: "#78909c",
            progress: "#69f0ae",
        },
    ),
    (
        "bride",
        Preset {
            accent: "#e0e0e0",
            highlight: "#80d8ff",
            error: "#ff5252",
            muted: "#9e9e9e",
            progress: "#f5f5f5",
        },
    ),
    (
        "wolfman",
        Preset {
            accent: "#ffd54f",
            highlight: "#ff8f00",
            error: "#e53935",
            muted: "#757575",
            progress: "#ffca28",
        },
    ),
    (
        "mummy",
        Preset {
            accent: "#d7ccc8",
            highlight: "#ffd54f",
            error: "#e53935",
            muted: "#8d6e63",
            progress: "#ffca28",
        },
    ),
    (
        "invisible",
        Preset {
            accent: "#424242",
            highlight: "#616161",
            error: "#b71c1c",
            muted: "#212121",
            progress: "#9e9e9e",
        },
    ),
    (
        "lagoon",
        Preset {
            accent: "#00897b",
            highlight: "#4db6ac",
            error: "#ff5252",
            muted: "#37474f",
            progress: "#64ffda",
        },
    ),
    (
        "thriller",
        Preset {
            accent: "#d00000",
            highlight: "#ffb700",
            error: "#ff1744",
            muted: "#455a64",
            progress: "#ff6d00",
        },
    ),
];

/// Nombres de los presets disponibles (para mensajes de ayuda).
pub fn preset_names() -> Vec<&'static str> {
    PRESETS.iter().map(|(name, _)| *name).collect()
}

/// Busca un preset por nombre (insensible a mayúsculas y espacios).
pub fn find_preset(name: &str) -> Option<ThemeConfig> {
    let name = name.trim().to_lowercase();
    PRESETS
        .iter()
        .find(|(preset, _)| *preset == name)
        .map(|(_, preset)| (*preset).into_config())
}

/// Directorio de temas del usuario (RF-71): `<config_dir>/shellmusic/themes`.
pub fn user_themes_dir() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|dir| dir.join("shellmusic").join("themes"))
}

/// Carga los temas de usuario (`<nombre>.toml` con los campos de
/// [`ThemeConfig`]). Los archivos que no parsean se reportan como avisos
/// (ruta + causa) y no impiden cargar el resto.
pub fn load_user_themes() -> (Vec<(String, ThemeConfig)>, Vec<String>) {
    match user_themes_dir() {
        Some(dir) => load_user_themes_from(&dir),
        None => (Vec::new(), Vec::new()),
    }
}

/// Versión con ruta explícita (tests; ver `load_user_themes`).
pub fn load_user_themes_from(dir: &std::path::Path) -> (Vec<(String, ThemeConfig)>, Vec<String>) {
    let mut themes = Vec::new();
    let mut warnings = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return (themes, warnings),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "toml") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<ThemeConfig>(&text) {
                Ok(config) => themes.push((name.to_lowercase(), config)),
                Err(err) => warnings.push(format!("{}: {err}", path.display())),
            },
            Err(err) => warnings.push(format!("{}: {err}", path.display())),
        }
    }
    themes.sort_by(|a, b| a.0.cmp(&b.0));
    (themes, warnings)
}

/// Busca un tema por nombre (RF-71): primero los presets empaquetados,
/// después los temas de usuario. Devuelve la config y el nombre canónico
/// (minúsculas).
pub fn find_theme(name: &str) -> Option<(ThemeConfig, String)> {
    let wanted = name.trim().to_lowercase();
    if let Some(config) = find_preset(&wanted) {
        return Some((config, wanted));
    }
    let (themes, _) = load_user_themes();
    find_user_theme(&themes, &wanted)
}

/// Busca un nombre entre temas de usuario cargados (lo separa de
/// `find_theme` para poder probarlo con un directorio temporal).
fn find_user_theme(
    themes: &[(String, ThemeConfig)],
    wanted: &str,
) -> Option<(ThemeConfig, String)> {
    themes
        .iter()
        .find(|(name, _)| name == wanted)
        .map(|(name, config)| (config.clone(), name.clone()))
}

/// Nombres de todos los temas disponibles (presets y de usuario), para el
/// selector del menú de opciones (RF-70).
pub fn all_theme_names() -> Vec<String> {
    let mut names: Vec<String> = PRESETS.iter().map(|(name, _)| (*name).to_string()).collect();
    names.extend(load_user_themes().0.into_iter().map(|(name, _)| name));
    names
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: Color::Cyan,
            highlight: Color::Yellow,
            error: Color::Red,
            muted: Color::DarkGray,
            progress: Color::Green,
        }
    }
}

impl Theme {
    /// Resuelve la configuración a colores; los valores inválidos caen al
    /// color por defecto (sin panic) y se reportan en un aviso para la UI.
    pub fn resolve(config: &ThemeConfig) -> (Self, Option<String>) {
        let default = Self::default();
        let mut invalid: Vec<String> = Vec::new();
        let mut parse = |value: &str, name: &str, fallback: Color| {
            match Color::from_str(value.trim()) {
                Ok(color) => color,
                Err(_) => {
                    invalid.push(format!("{name}={value:?}"));
                    fallback
                }
            }
        };
        let theme = Self {
            accent: parse(&config.accent, "accent", default.accent),
            highlight: parse(&config.highlight, "highlight", default.highlight),
            error: parse(&config.error, "error", default.error),
            muted: parse(&config.muted, "muted", default.muted),
            progress: parse(&config.progress, "progress", default.progress),
        };
        let warning = (!invalid.is_empty()).then(|| {
            format!(
                "tema: colores inválidos ({}); se usan los valores por defecto",
                invalid.join(", ")
            )
        });
        (theme, warning)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_resolves_to_default_theme() {
        let (theme, warning) = Theme::resolve(&ThemeConfig::default());
        assert_eq!(theme, Theme::default());
        assert!(warning.is_none());
    }

    #[test]
    fn every_preset_resolves_without_warnings() {
        for (name, preset) in PRESETS {
            let (theme, warning) = Theme::resolve(&preset.into_config());
            assert!(warning.is_none(), "preset {name}: {warning:?}");
            assert_ne!(theme, Theme::default(), "preset {name} debe diferir del default");
        }
    }

    #[test]
    fn find_preset_matches_case_insensitive() {
        assert!(find_preset("dracula").is_some());
        assert!(find_preset("DRACULA").is_some());
        assert!(find_preset(" Dracula ").is_some());
        assert!(find_preset("inexistente").is_none());
    }

    #[test]
    fn user_themes_load_from_dir_with_warnings() {
        let dir = std::env::temp_dir().join(format!(
            "shellmusic-themes-{}-{:016x}",
            std::process::id(),
            crate::state_file::random_suffix()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("mi-tema.toml"), "accent = \"#ff00ff\"\n").unwrap();
        std::fs::write(dir.join("roto.toml"), "no es toml [[[[").unwrap();
        std::fs::write(dir.join("ignorado.txt"), "accent = red").unwrap();
        let (themes, warnings) = load_user_themes_from(&dir);
        assert_eq!(themes.len(), 1, "temas: {themes:?}");
        assert_eq!(themes[0].0, "mi-tema");
        assert_eq!(themes[0].1.accent, "#ff00ff");
        assert_eq!(warnings.len(), 1, "avisos: {warnings:?}");
        assert!(warnings[0].contains("roto.toml"), "aviso: {:?}", warnings[0]);
        // El preset sigue encontrándose por nombre.
        assert!(find_theme("DRACULA").is_some());
        // Y el tema de usuario se resuelve sobre la lista cargada (insensible
        // a mayúsculas); find_theme lee el directorio real, no el temporal.
        let (config, name) = find_user_theme(&themes, "mi-tema").expect("tema de usuario");
        assert_eq!(name, "mi-tema");
        assert_eq!(config.accent, "#ff00ff");
        assert!(find_user_theme(&themes, "inexistente").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn user_themes_missing_dir_is_empty() {
        let dir = std::env::temp_dir().join("shellmusic-themes-que-no-existe-xyz");
        let (themes, warnings) = load_user_themes_from(&dir);
        assert!(themes.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn accepts_names_hex_and_indices() {
        let config = ThemeConfig {
            accent: "magenta".to_string(),
            highlight: "#fab387".to_string(),
            error: "196".to_string(),
            muted: "Dark Gray".to_string(),
            progress: "lightgreen".to_string(),
        };
        let (theme, warning) = Theme::resolve(&config);
        assert_eq!(theme.accent, Color::Magenta);
        assert_eq!(theme.highlight, Color::Rgb(0xfa, 0xb3, 0x87));
        assert_eq!(theme.error, Color::Indexed(196));
        assert_eq!(theme.muted, Color::DarkGray);
        assert_eq!(theme.progress, Color::LightGreen);
        assert!(warning.is_none());
    }

    #[test]
    fn invalid_values_fall_back_with_warning() {
        let config = ThemeConfig {
            accent: "inventado".to_string(),
            ..ThemeConfig::default()
        };
        let (theme, warning) = Theme::resolve(&config);
        assert_eq!(theme.accent, Theme::default().accent);
        assert_eq!(theme.highlight, Theme::default().highlight);
        let warning = warning.expect("debe avisar del valor inválido");
        assert!(warning.contains("accent"), "{warning}");
    }
}
