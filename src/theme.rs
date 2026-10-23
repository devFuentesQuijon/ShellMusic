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
