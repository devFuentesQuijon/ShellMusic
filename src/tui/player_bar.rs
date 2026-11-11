use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Gauge;

use super::fmt_duration;
use crate::app::{App, SleepTimer};

/// Etiqueta del temporizador de apagado para la barra: `⏳mm:ss` o `⏳fin pista`.
pub(crate) fn sleep_label(timer: Option<SleepTimer>) -> String {
    match timer {
        Some(SleepTimer::AfterTrack) => "  ⏳fin pista".to_string(),
        Some(SleepTimer::At(deadline)) => {
            let secs = deadline
                .saturating_duration_since(std::time::Instant::now())
                .as_secs();
            format!("  ⏳{}:{:02}", secs / 60, secs % 60)
        }
        None => String::new(),
    }
}

/// Etiqueta «a continuación» para la barra: título de la siguiente pista y
/// duración restante de la cola (sumando solo pistas con duración conocida).
/// Vacía cuando la actual es la última o no hay reproducción.
fn up_next_label(app: &App) -> String {
    let Some(current) = app.queue.current else {
        return String::new();
    };
    let Some(next) = app.queue.tracks.get(current + 1) else {
        return String::new();
    };
    let rest: std::time::Duration = app.queue.tracks[current + 1..]
        .iter()
        .filter_map(|track| track.duration)
        .sum();
    let secs = rest.as_secs();
    format!(
        "  → {}  +{}:{:02}",
        truncate(&next.title, 20),
        secs / 60,
        secs % 60
    )
}

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
                "  {}/{}  cola {}/{}  vol:{}{}{}{}{}",
                fmt_duration(app.status.position),
                fmt_duration(app.status.duration.or(track.duration)),
                position,
                app.queue.len(),
                app.status.volume,
                app.repeat.bar_label(),
                radio,
                sleep_label(app.sleep_timer),
                up_next_label(app)
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
                    "Sin reproducción · vol:{}{}{}{}",
                    app.status.volume,
                    app.repeat.bar_label(),
                    radio,
                    sleep_label(app.sleep_timer)
                ),
            )
        }
    };
    if app.ui_style == crate::config::UiStyle::Modern {
        draw_modern(frame, app, area, ratio, &label);
        return;
    }
    let gauge = Gauge::default()
        .block(super::panel_block(app, "Reproductor".to_string(), false))
        .gauge_style(Style::new().fg(app.theme.progress))
        .ratio(ratio)
        .label(label);
    frame.render_widget(gauge, area);
}

/// Barra del estilo moderno (RF-69): «█» con degradado progress→accent para
/// lo sonado y «░» en muted para lo que falta; la etiqueta se superpone
/// centrada, como hace `Gauge`.
fn draw_modern(frame: &mut Frame, app: &App, area: Rect, ratio: f64, label: &str) {
    let block = super::panel_block(app, "Reproductor".to_string(), false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = inner.width as usize;
    let filled = (ratio * width as f64).round() as usize;
    frame.render_widget(
        ratatui::widgets::Paragraph::new(super::gradient_bar(
            width,
            filled,
            app.theme.progress,
            app.theme.accent,
            app.theme.muted,
        )),
        inner,
    );
    let label_width = label.chars().count() as u16;
    let label_area = Rect {
        x: inner.x + inner.width.saturating_sub(label_width) / 2,
        y: inner.y,
        width: label_width.min(inner.width),
        height: 1,
    };
    frame.render_widget(
        ratatui::widgets::Paragraph::new(label.to_string()),
        label_area,
    );
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
