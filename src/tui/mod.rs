pub mod command;
pub mod favorites;
pub mod help;
pub mod nowplaying;
pub mod options;
pub mod player_bar;
pub mod playlists;
pub mod queue;
pub mod search;
pub mod stats;

use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};

use crate::app::{App, Focus, Overlay};
use crate::config::UiStyle;

const HELP_SEARCH: &str = "Enter buscar · Tab panel · Esc resultados · Ctrl+C salir";
const HELP_RESULTS: &str = "Enter reproducir · a cola · f favorito · o artista · y copiar · ←/→ seek · 0-9 % · l repetir · ? ayuda · q salir";
const HELP_QUEUE: &str = "Enter reproducir · d eliminar · c ir a actual · / filtrar · J/K mover · y copiar · ←/→ seek · l repetir · ? ayuda · q salir";
const HELP_FAVORITES: &str =
    "Enter reproducir · a cola · d/f quitar · y copiar · ←/→ seek · l repetir · ? ayuda · q salir";

pub fn draw(frame: &mut Frame, app: &mut App) {
    let [search_area, middle_area, status_area, player_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(3),
    ])
    .areas(frame.area());
    // Tres columnas: en 80 columnas quedan ~36/22/22; los títulos largos se
    // truncan pero todo sigue alcanzable por teclado (RNF-3).
    let [results_area, queue_area, favorites_area] = Layout::horizontal([
        Constraint::Percentage(40),
        Constraint::Percentage(30),
        Constraint::Percentage(30),
    ])
    .areas(middle_area);
    // Áreas para el hit-testing del ratón: click en barra = seek, click en
    // panel = foco, click en fila = selección.
    app.areas = crate::app::PanelAreas {
        results: results_area,
        queue: queue_area,
        favorites: favorites_area,
        player: player_area,
    };
    // Filas útiles de los paneles (menos el borde) para PgUp/PgDn.
    app.list_page_size = middle_area.height.saturating_sub(2).max(1) as usize;
    search::draw_input(frame, app, search_area);
    search::draw_results(frame, app, results_area);
    queue::draw(frame, app, queue_area);
    favorites::draw(frame, app, favorites_area);
    draw_status(frame, app, status_area);
    player_bar::draw(frame, app, player_area);
    playlists::draw(frame, app);
    help::draw(frame, app);
    command::draw(frame, app);
    options::draw(frame, app);
    stats::draw(frame, app);
    stats::draw_export(frame, app);
    stats::draw_path_input(frame, app);
    nowplaying::draw_lyrics(frame, app);
    nowplaying::draw_now_playing(frame, app);
}

fn draw_status(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let (text, style) = match &app.message {
        Some(msg) if msg.is_error => (msg.text.clone(), Style::new().fg(app.theme.error)),
        Some(msg) => (msg.text.clone(), Style::new().fg(app.theme.highlight)),
        None => (
            match app.overlay {
                Overlay::Closed => match app.focus {
                    Focus::Searching => HELP_SEARCH,
                    Focus::Browsing => HELP_RESULTS,
                    Focus::Queue => HELP_QUEUE,
                    Focus::Favorites => HELP_FAVORITES,
                },
                Overlay::Help => "ayuda: Esc cierra",
                Overlay::Stats => "estadísticas: Tab vista · Enter reproducir · Esc cierra",
                Overlay::Lyrics => "letra: f seguir on/off · ↑/↓ scroll · Esc cierra",
                Overlay::NowPlaying => "sonando ahora: Esc cierra",
                Overlay::Export | Overlay::PathInput(_) => {
                    "exportar/importar: Enter confirmar · Esc cancelar"
                }
                Overlay::Command => "comandos: Enter ejecutar · Esc cerrar",
                Overlay::Options => "opciones: ←/→ cambiar · Tab categoría · Esc cerrar",
                _ => "playlists: las teclas se muestran dentro del panel",
            }
            .to_string(),
            Style::new().fg(app.theme.muted),
        ),
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
}

/// Tipo de borde según el estilo UI (RF-69): redondeado en el moderno
/// (inspirado en btop), cuadrado en el clásico.
pub(super) fn border_type(style: UiStyle) -> BorderType {
    match style {
        UiStyle::Classic => BorderType::Plain,
        UiStyle::Modern => BorderType::Rounded,
    }
}

/// Bloque estándar de panel: título, borde resaltado con el foco y tipo de
/// borde según el estilo UI.
pub(super) fn panel_block(app: &App, title: String, focused: bool) -> Block<'static> {
    let border_style = if focused {
        Style::new().fg(app.theme.accent)
    } else {
        Style::default()
    };
    Block::bordered()
        .title(title)
        .border_style(border_style)
        .border_type(border_type(app.ui_style))
}

/// RGB aproximado de un `Color` de ratatui (base de los degradados): los
/// nombres usan la tabla ANSI y los índices la paleta xterm-256.
pub(super) fn color_to_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Indexed(i) => indexed_to_rgb(i),
        Color::Black => (0, 0, 0),
        Color::Red => (205, 0, 0),
        Color::Green => (0, 205, 0),
        Color::Yellow => (205, 205, 0),
        Color::Blue => (0, 0, 238),
        Color::Magenta => (205, 0, 205),
        Color::Cyan => (0, 205, 205),
        Color::Gray => (229, 229, 229),
        Color::DarkGray => (127, 127, 127),
        Color::LightRed => (255, 0, 0),
        Color::LightGreen => (0, 255, 0),
        Color::LightYellow => (255, 255, 0),
        Color::LightBlue => (92, 92, 255),
        Color::LightMagenta => (255, 0, 255),
        Color::LightCyan => (0, 255, 255),
        Color::White => (255, 255, 255),
        Color::Reset => (128, 128, 128),
    }
}

fn indexed_to_rgb(i: u8) -> (u8, u8, u8) {
    const ANSI16: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (205, 0, 0),
        (0, 205, 0),
        (205, 205, 0),
        (0, 0, 238),
        (205, 0, 205),
        (0, 205, 205),
        (229, 229, 229),
        (127, 127, 127),
        (255, 0, 0),
        (0, 255, 0),
        (255, 255, 0),
        (92, 92, 255),
        (255, 0, 255),
        (0, 255, 255),
        (255, 255, 255),
    ];
    match i {
        0..=15 => ANSI16[i as usize],
        // Cubo 6×6×6: niveles 0-5 escalados a 0-255 en pasos de 51.
        16..=231 => {
            let n = i - 16;
            ((n / 36) * 51, ((n % 36) / 6) * 51, (n % 6) * 51)
        }
        // Escala de grises: 232 = #080808, +10 por paso.
        232..=255 => {
            let v = 8 + (i - 232) * 10;
            (v, v, v)
        }
    }
}

/// Interpola dos colores en `t` (0.0-1.0) para los degradados del estilo
/// moderno.
pub(super) fn lerp_color(from: Color, to: Color, t: f32) -> Color {
    let (fr, fg, fb) = color_to_rgb(from);
    let (tr, tg, tb) = color_to_rgb(to);
    let lerp = |a: u8, b: u8| {
        (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)).round() as u8
    };
    Color::Rgb(lerp(fr, tr), lerp(fg, tg), lerp(fb, tb))
}

/// Línea de barra con degradado (estilo moderno): las primeras `filled`
/// celdas son «█» del degradado `from`→`to` y el resto «░» en `empty`.
pub(super) fn gradient_bar(
    width: usize,
    filled: usize,
    from: Color,
    to: Color,
    empty: Color,
) -> Line<'static> {
    let width = width.max(1);
    let spans: Vec<Span> = (0..width)
        .map(|i| {
            if i < filled {
                let t = i as f32 / (width - 1).max(1) as f32;
                Span::styled("█", Style::new().fg(lerp_color(from, to, t)))
            } else {
                Span::styled("░", Style::new().fg(empty))
            }
        })
        .collect();
    Line::from(spans)
}

pub fn fmt_duration(duration: Option<Duration>) -> String {
    match duration {
        Some(d) => {
            let secs = d.as_secs();
            format!("{}:{:02}", secs / 60, secs % 60)
        }
        None => "--:--".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_to_rgb_covers_names_and_palette() {
        assert_eq!(color_to_rgb(Color::Rgb(1, 2, 3)), (1, 2, 3));
        assert_eq!(color_to_rgb(Color::Red), (205, 0, 0));
        assert_eq!(color_to_rgb(Color::DarkGray), (127, 127, 127));
        // xterm-256: 196 = rojo del cubo (5,0,0) · 51.
        assert_eq!(color_to_rgb(Color::Indexed(196)), (255, 0, 0));
        // Escala de grises: 232 = 8, 255 = 238.
        assert_eq!(color_to_rgb(Color::Indexed(232)), (8, 8, 8));
        assert_eq!(color_to_rgb(Color::Indexed(255)), (238, 238, 238));
    }

    #[test]
    fn lerp_interpolates_and_clamps() {
        let from = Color::Rgb(0, 0, 0);
        let to = Color::Rgb(100, 200, 50);
        assert_eq!(lerp_color(from, to, 0.0), Color::Rgb(0, 0, 0));
        assert_eq!(lerp_color(from, to, 1.0), Color::Rgb(100, 200, 50));
        assert_eq!(lerp_color(from, to, 0.5), Color::Rgb(50, 100, 25));
        assert_eq!(lerp_color(from, to, 2.0), to, "se satura");
        assert_eq!(lerp_color(from, to, -1.0), from, "y por abajo");
    }

    #[test]
    fn gradient_bar_marks_filled_cells() {
        let line = gradient_bar(10, 4, Color::Black, Color::White, Color::DarkGray);
        assert_eq!(line.spans.len(), 10);
        for (i, span) in line.spans.iter().enumerate() {
            let expected: &str = if i < 4 { "█" } else { "░" };
            assert_eq!(span.content.as_ref(), expected);
        }
    }

    #[test]
    fn border_type_follows_ui_style() {
        assert_eq!(border_type(UiStyle::Classic), BorderType::Plain);
        assert_eq!(border_type(UiStyle::Modern), BorderType::Rounded);
    }
}
