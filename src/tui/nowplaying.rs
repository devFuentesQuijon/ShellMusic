use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui_image::StatefulImage;

use super::fmt_duration;
use super::playlists::{block, centered};
use crate::app::{App, Overlay};

/// Overlay de letras (tecla L). Con letra sincronizada de la pista que suena,
/// la línea activa se resalta y el scroll la sigue; cualquier scroll manual
/// cede el control (`f` lo recupera).
pub fn draw_lyrics(frame: &mut Frame, app: &mut App) {
    if !matches!(app.overlay, Overlay::Lyrics) {
        return;
    }
    let area = centered(frame.area(), 70, 80);
    app.overlay_page_size = area.height.saturating_sub(3).max(1) as usize;
    frame.render_widget(Clear, area);

    // La vista sincronizada solo sigue la reproducción si la letra es de la
    // pista actual; para otra pista (o sin posición) se muestra plana.
    let synced = app.lyrics_synced.as_ref().filter(|_| {
        app.lyrics_track
            .as_ref()
            .zip(app.queue.current_track())
            .is_some_and(|(key, track)| key.0 == track.provider && key.1 == track.id)
    });
    let Some(lines) = synced else {
        let text = app.lyrics_text.as_deref().unwrap_or("buscando letra…");
        let paragraph = Paragraph::new(text)
            .block(block(
                format!("Letra: {}", app.lyrics_title),
                app.theme.accent,
            ))
            .wrap(Wrap { trim: true })
            .scroll((app.lyrics_scroll, 0));
        frame.render_widget(paragraph, area);
        draw_hint(
            frame,
            area,
            "↑/↓ scroll · PgUp/PgDn página · g/G inicio/fin · Esc cerrar",
            app,
        );
        return;
    };

    let position = app.status.position.map_or(0.0, |pos| pos.as_secs_f64());
    let active = crate::lyrics::active_line(lines, position);
    let rows: Vec<Line> = lines
        .iter()
        .enumerate()
        .map(|(i, (_, text))| {
            let style = match active {
                Some(a) if i == a => Style::new()
                    .fg(app.theme.accent)
                    .add_modifier(Modifier::BOLD),
                Some(a) if i < a => Style::new().fg(app.theme.muted),
                _ => Style::new(),
            };
            let marker = if active == Some(i) { "♪ " } else { "  " };
            Line::styled(format!("{marker}{text}"), style)
        })
        .collect();
    // Auto-seguimiento: la línea activa queda en el tercio superior; con
    // scroll manual manda `lyrics_scroll`.
    let scroll = if app.lyrics_follow {
        active
            .map_or(0, |a| a.saturating_sub(app.overlay_page_size / 3))
            .min(lines.len().saturating_sub(1)) as u16
    } else {
        app.lyrics_scroll
    };
    let paragraph = Paragraph::new(rows)
        .block(block(
            format!("Letra: {} (sincronizada)", app.lyrics_title),
            app.theme.accent,
        ))
        .scroll((scroll, 0));
    frame.render_widget(paragraph, area);
    draw_hint(
        frame,
        area,
        "♪ línea actual · f seguir on/off · ↑/↓ scroll manual · Esc cerrar",
        app,
    );
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
            Paragraph::new("Nada en reproducción")
                .block(block("Sonando ahora".to_string(), accent)),
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
            if app.status.paused {
                "pausada"
            } else {
                "sonando"
            }
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
            Paragraph::new(
                if track.provider == crate::providers::ProviderKind::Youtube {
                    "cargando carátula…"
                } else {
                    "sin carátula (SoundCloud)"
                },
            )
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
