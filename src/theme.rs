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
