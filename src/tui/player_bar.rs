use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Gauge};

use super::fmt_duration;
use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let (ratio, label) = match app.queue.current_track() {
        Some(track) => {
            let ratio = match (app.status.position, app.status.duration) {
                (Some(pos), Some(dur)) if !dur.is_zero() => {
                    (pos.as_secs_f64() / dur.as_secs_f64()).clamp(0.0, 1.0)
                }
                _ => 0.0,
            };
            let icon = if app.status.paused { "⏸" } else { "▶" };
            let position = app.queue.current.map_or(0, |index| index + 1);
            let radio = if app.radio {
                format!("  radio:ON·{}", app.radio_mode)
            } else {
                String::new()
            };
            let meta = format!(
                "  {}/{}  cola {}/{}  vol:{}{}{}",
                fmt_duration(app.status.position),
                fmt_duration(app.status.duration.or(track.duration)),
                position,
                app.queue.len(),
                app.status.volume,
                app.repeat.bar_label(),
                radio
            );
            // Los títulos pueden ser larguísimos: se truncan para que los
            // metadatos (progreso, cola, volumen, repetición, radio)
            // quepan siempre en la barra.
            let inner = area.width.saturating_sub(2) as usize;
            let budget = inner.saturating_sub(3 + meta.chars().count());
            let name = truncate(&format!("{} — {}", track.title, track.artist), budget);
            let label = format!("{icon} {name}{meta}");
            (ratio, label)
        }
        None => {
            let radio = if app.radio {
                format!(" · radio:ON·{}", app.radio_mode)
            } else {
                String::new()
            };
            (
                0.0,
                format!(
                    "Sin reproducción · vol:{}{}{}",
                    app.status.volume,
                    app.repeat.bar_label(),
                    radio
                ),
            )
        }
    };
    let gauge = Gauge::default()
        .block(Block::bordered().title("Reproductor"))
        .gauge_style(Style::new().fg(app.theme.progress))
        .ratio(ratio)
        .label(label);
    frame.render_widget(gauge, area);
}

/// Corta `text` a `max` caracteres añadiendo '…' si sobra.
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}
