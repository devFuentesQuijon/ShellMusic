use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui_image::StatefulImage;

use super::playlists::{block, centered};
use super::fmt_duration;
use crate::app::{App, Overlay};

/// Overlay de letras (tecla L) con scroll.
pub fn draw_lyrics(frame: &mut Frame, app: &mut App) {
    if !matches!(app.overlay, Overlay::Lyrics) {
        return;
    }
    let area = centered(frame.area(), 70, 80);
    app.overlay_page_size = area.height.saturating_sub(3).max(1) as usize;
    frame.render_widget(Clear, area);
    let text = app.lyrics_text.as_deref().unwrap_or("buscando letra…");
    let paragraph = Paragraph::new(text)
        .block(block(format!("Letra: {}", app.lyrics_title), app.theme.accent))
        .wrap(Wrap { trim: true })
        .scroll((app.lyrics_scroll, 0));
    frame.render_widget(paragraph, area);
    draw_hint(frame, area, "↑/↓ scroll · PgUp/PgDn página · g/G inicio/fin · Esc cerrar", app);
}

/// Panel «sonando ahora» (tecla i): metadata ampliada y carátula.
pub fn draw_now_playing(frame: &mut Frame, app: &mut App) {
    if !matches!(app.overlay, Overlay::NowPlaying) {
        return;
    }
    let area = centered(frame.area(), 70, 60);
    frame.render_widget(Clear, area);
    let accent = app.theme.accent;

    let Some(track) = app.queue.current_track() else {
        frame.render_widget(
            Paragraph::new("Nada en reproducción").block(block("Sonando ahora".to_string(), accent)),
            area,
        );
        return;
    };

    let [meta_area, cover_area] =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(area);

    let plays = app
        .stats
        .entries
        .iter()
        .find(|e| e.provider == track.provider && e.id == track.id)
        .map_or(0, |e| e.plays);
    let position = app.queue.current.map_or(0, |i| i + 1);
    let lines = [
        format!("Título:   {}", track.title),
        format!("Artista:  {}", track.artist),
        format!("Fuente:   {}", track.provider),
        format!(
            "Duración: {}  (posición {})",
            fmt_duration(app.status.duration.or(track.duration)),
            fmt_duration(app.status.position)
        ),
        format!(
            "Estado:   {}",
            if app.status.paused { "pausada" } else { "sonando" }
        ),
        format!("Volumen:  {}", app.status.volume),
        format!("Cola:     {}/{}", position, app.queue.len()),
        format!("Escuchas: {plays}"),
    ];
    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .block(block("Sonando ahora".to_string(), accent))
            .wrap(Wrap { trim: false }),
        meta_area,
    );

    // Carátula solo si corresponde a la pista actual (puede ser de la anterior).
    let cover_valid = app
        .cover_for
        .as_ref()
        .is_some_and(|(provider, id)| *provider == track.provider && *id == track.id);
    if cover_valid && let Some(cover) = &mut app.cover {
        let image = StatefulImage::new();
        frame.render_stateful_widget(image, cover_area, cover);
    } else {
        frame.render_widget(
            Paragraph::new(if track.provider == crate::providers::ProviderKind::Youtube {
                "cargando carátula…"
            } else {
                "sin carátula (SoundCloud)"
            })
            .style(Style::new().fg(app.theme.muted)),
            cover_area,
        );
    }
    draw_hint(frame, area, "Esc / i cerrar", app);
}

fn draw_hint(frame: &mut Frame, area: Rect, hint: &str, app: &App) {
    if area.height < 3 {
        return;
    }
    let hint_area = Rect {
        x: area.x + 1,
        y: area.y + area.height - 2,
        width: area.width.saturating_sub(2),
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(hint).style(Style::new().fg(app.theme.muted)),
        hint_area,
    );
}
