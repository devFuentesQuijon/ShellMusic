use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
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
            let label = format!(
                "{icon} {} — {}  {}/{}  cola {}/{}  vol:{}",
                track.title,
                track.artist,
                fmt_duration(app.status.position),
                fmt_duration(app.status.duration.or(track.duration)),
                position,
                app.queue.len(),
                app.status.volume
            );
            (ratio, label)
        }
        None => (
            0.0,
            format!("Sin reproducción · vol:{}", app.status.volume),
        ),
    };
    let gauge = Gauge::default()
        .block(Block::bordered().title("Reproductor"))
        .gauge_style(Style::new().fg(Color::Green))
        .ratio(ratio)
        .label(label);
    frame.render_widget(gauge, area);
}
