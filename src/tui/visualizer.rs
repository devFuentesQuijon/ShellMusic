use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::app::App;

const PARTIAL: [char; 8] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇'];

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let Some(visualizer) = &app.visualizer else {
        return;
    };
    let bands = visualizer.bands();
    if bands.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }
    let rows = area.height as usize;
    let width = area.width as usize;
    let count = bands.len().min(width);
    let step = width / count;
    let levels: Vec<f32> = (0..count)
        .map(|i| {
            // Si hay más bandas que columnas, toma el máximo del grupo.
            let lo = i * bands.len() / count;
            let hi = ((i + 1) * bands.len() / count).max(lo + 1);
            bands[lo..hi].iter().cloned().fold(0.0, f32::max)
        })
        .collect();

    let mut lines = Vec::with_capacity(rows);
    for row in (0..rows).rev() {
        let mut spans = Vec::with_capacity(count);
        for &level in &levels {
            let height = level * rows as f32;
            let cell = height - row as f32;
            let (ch, shade) = if cell >= 1.0 {
                ('█', 1.0)
            } else if cell > 0.0 {
                (PARTIAL[(cell * 8.0) as usize], cell)
            } else {
                (' ', 0.0)
            };
            spans.push(Span::styled(
                ch.to_string().repeat(step),
                Style::new().fg(level_color(row, rows, shade)),
            ));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(ratatui::widgets::Paragraph::new(lines), area);
}

/// Gradiente vertical: verde abajo, amarillo en medio, rojo arriba;
/// la fila parcial se atenúa según su fracción.
fn level_color(row: usize, rows: usize, shade: f32) -> Color {
    let t = row as f32 / rows.max(1) as f32;
    let (r, g) = if t < 0.5 {
        ((t * 2.0 * 255.0) as u8, 200)
    } else {
        (255, ((1.0 - (t - 0.5) * 2.0) * 200.0) as u8)
    };
    let dim = 0.45 + 0.55 * shade;
    Color::Rgb((r as f32 * dim) as u8, (g as f32 * dim) as u8, 40)
}
