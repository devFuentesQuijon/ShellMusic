//! Integración con el sistema operativo: notificaciones de escritorio al
//! cambiar de pista (notify-rust) y controles multimedia MPRIS en Linux
//! (souvlaki, backend zbus): teclas multimedia, widgets del escritorio y
//! KDE Connect. En macOS/Windows los media controls necesitan una ventana o
//! event loop de GUI, incompatibles con una TUI: allí solo hay notificaciones.

use std::time::Duration;

use tokio::sync::mpsc;

use crate::app::App;
use crate::config::Integrations;
use crate::providers::{ProviderKind, Track};

#[cfg(target_os = "linux")]
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig};

/// Evento de media controls hacia el bucle de la app. Se define un tipo
/// propio para que el módulo compile igual en todas las plataformas.
#[derive(Debug, Clone, PartialEq)]
pub enum MediaCommand {
    Play,
    Pause,
    Toggle,
    Stop,
    Next,
    Previous,
    SeekForward(Duration),
    SeekBackward(Duration),
    SetPosition(Duration),
    /// Volumen 0.0-1.0 según MPRIS.
    SetVolume(f64),
}

pub struct SystemIntegration {
    notifications: bool,
    #[cfg(target_os = "linux")]
    controls: Option<MediaControls>,
    /// Presencia en Discord: se alimenta en `update` con cambios de pista.
    discord: Option<crate::discord::PresenceSender>,
    /// Última pista publicada (para no repetir metadata/notificación).
    last_track: Option<(ProviderKind, String)>,
    last_paused: bool,
    /// Aviso de arranque si los media controls no pudieron iniciarse.
    pub startup_warning: Option<String>,
}

impl SystemIntegration {
    #[cfg(target_os = "linux")]
    pub fn new(
        config: &Integrations,
        tx: mpsc::UnboundedSender<MediaCommand>,
    ) -> Self {
        let mut warning = None;
        let controls = if config.media_controls {
            let platform = PlatformConfig {
                dbus_name: "shellmusic",
                display_name: "ShellMusic",
                hwnd: None,
            };
            match MediaControls::new(platform) {
                Ok(mut controls) => {
                    let tx = tx.clone();
                    match controls.attach(move |event| {
                        if let Some(command) = map_event(event) {
                            let _ = tx.send(command);
                        }
                    }) {
                        Ok(()) => Some(controls),
                        Err(err) => {
                            warning = Some(format!("media controls no disponibles: {err}"));
                            None
                        }
                    }
                }
                Err(err) => {
                    warning = Some(format!("media controls no disponibles: {err}"));
                    None
                }
            }
        } else {
            None
        };
        let (discord, discord_warning) =
            crate::discord::spawn(config.discord, &config.discord_client_id);
        Self {
            notifications: config.notifications,
            controls,
            discord,
            last_track: None,
            last_paused: false,
            startup_warning: warning.or(discord_warning),
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn new(
        config: &Integrations,
        _tx: mpsc::UnboundedSender<MediaCommand>,
    ) -> Self {
        let (discord, discord_warning) =
            crate::discord::spawn(config.discord, &config.discord_client_id);
        Self {
            notifications: config.notifications,
            discord,
            last_track: None,
            last_paused: false,
            startup_warning: discord_warning,
        }
    }

    /// Publica metadata/notificación solo cuando cambia la pista o la pausa.
    /// La notificación lleva la carátula cuando ya está en disco; si aún se
    /// descarga (o falla), queda pendiente y se resuelve en el próximo
    /// update, cuando la carátula está lista o el fetch terminó en error.
    pub fn update(&mut self, app: &mut App) {
        // Resolución de la notificación pendiente: el fetch de carátula
        // terminó (bien o mal) o la pista cambió antes de que llegara.
        if let Some(pending) = app.notify_pending.clone() {
            let current = app
                .queue
                .current_track()
                .map(|t| (t.provider, t.id.clone()));
            let ready = app.cover_ready_for.as_ref() == Some(&pending);
            let failed = app.cover_failed_for.as_ref() == Some(&pending);
            if ready || failed || current.as_ref() != Some(&pending) {
                app.notify_pending = None;
                if current == Some(pending) && self.notifications
                    && let Some(track) = app.queue.current_track().cloned()
                {
                    notify(&track, ready.then(|| app.cover_png_path.clone()));
                }
            }
        }

        let track = app.queue.current_track();
        let key = track.map(|t| (t.provider, t.id.clone()));
        let paused = app.status.paused;
        if key == self.last_track && paused == self.last_paused {
            return;
        }
        let track_changed = key != self.last_track;
        self.last_track = key;
        self.last_paused = paused;

        // Presencia en Discord: se publica con cada cambio de pista/pausa.
        // El inicio (temporizador del perfil) sale de `app.playing`, que
        // lleva la pista junto a su SystemTime de arranque.
        if let Some(sender) = &self.discord {
            let presence = match track {
                Some(track) if paused => crate::discord::Presence::Paused {
                    title: track.title.clone(),
                    artist: track.artist.clone(),
                },
                Some(track) => {
                    let start_unix = app
                        .playing
                        .as_ref()
                        .and_then(|(_, started)| {
                            started
                                .duration_since(std::time::UNIX_EPOCH)
                                .ok()
                                .map(|d| d.as_secs())
                        })
                        .unwrap_or_else(|| {
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map_or(0, |d| d.as_secs())
                        });
                    crate::discord::Presence::Playing {
                        title: track.title.clone(),
                        artist: track.artist.clone(),
                        start_unix,
                    }
                }
                None => crate::discord::Presence::Stopped,
            };
            sender.send(presence);
        }

        #[cfg(target_os = "linux")]
        if let Some(controls) = &mut self.controls {
            match track {
                Some(track) => {
                    let _ = controls.set_metadata(MediaMetadata {
                        title: Some(&track.title),
                        artist: Some(&track.artist),
                        duration: track.duration.or(app.status.duration),
                        ..Default::default()
                    });
                    let progress = app.status.position.map(MediaPosition);
                    let playback = if paused {
                        MediaPlayback::Paused { progress }
                    } else {
                        MediaPlayback::Playing { progress }
                    };
                    let _ = controls.set_playback(playback);
                }
                None => {
                    let _ = controls.set_playback(MediaPlayback::Stopped);
                }
            }
        }

        if track_changed && self.notifications
            && let Some(track) = track
        {
            let cover_key = (track.provider, track.id.clone());
            if app.cover_ready_for.as_ref() == Some(&cover_key) {
                // Carátula ya en disco: notificación con imagen ya mismo.
                notify(track, Some(app.cover_png_path.clone()));
            } else if track.provider == ProviderKind::Youtube
                && app.cover_failed_for.as_ref() != Some(&cover_key)
            {
                // Se está descargando (o aún no se pidió): se resuelve en el
                // próximo update cuando CoverDone fije el resultado.
                app.notify_pending = Some(cover_key);
            } else {
                // Sin carátula posible (no YouTube o fetch ya fallado): texto.
                notify(track, None);
            }
        }
    }
}

/// Traduce un evento de souvlaki a un comando propio (Linux).
#[cfg(target_os = "linux")]
fn map_event(event: MediaControlEvent) -> Option<MediaCommand> {
    use souvlaki::SeekDirection;
    match event {
        MediaControlEvent::Play => Some(MediaCommand::Play),
        MediaControlEvent::Pause => Some(MediaCommand::Pause),
        MediaControlEvent::Toggle => Some(MediaCommand::Toggle),
        MediaControlEvent::Stop => Some(MediaCommand::Stop),
        MediaControlEvent::Next => Some(MediaCommand::Next),
        MediaControlEvent::Previous => Some(MediaCommand::Previous),
        MediaControlEvent::Seek(SeekDirection::Forward) => {
            Some(MediaCommand::SeekForward(Duration::from_secs(5)))
        }
        MediaControlEvent::Seek(SeekDirection::Backward) => {
            Some(MediaCommand::SeekBackward(Duration::from_secs(5)))
        }
        MediaControlEvent::SeekBy(SeekDirection::Forward, delta) => {
            Some(MediaCommand::SeekForward(delta))
        }
        MediaControlEvent::SeekBy(SeekDirection::Backward, delta) => {
            Some(MediaCommand::SeekBackward(delta))
        }
        MediaControlEvent::SetPosition(MediaPosition(pos)) => Some(MediaCommand::SetPosition(pos)),
        MediaControlEvent::SetVolume(volume) => Some(MediaCommand::SetVolume(volume)),
        // Una TUI no puede elevar ventana; Quit/OpenUri no aplican.
        MediaControlEvent::Raise | MediaControlEvent::Quit | MediaControlEvent::OpenUri(_) => None,
    }
}

fn notify(track: &Track, image: Option<std::path::PathBuf>) {
    // El fallo de una notificación no debe ensuciar la UI: se ignora. La
    // imagen (carátula) solo la soportan bien los demonios de Linux: en
    // notify-rust el método `hint` solo existe en Linux/*BSD (dbus), así
    // que en macOS/Windows se compila sin hint y sale notificación de texto.
    let mut notification = notify_rust::Notification::new();
    notification
        .appname("ShellMusic")
        .summary(&track.title)
        .body(&track.artist);
    #[cfg(target_os = "linux")]
    if let Some(path) = image {
        notification.hint(notify_rust::Hint::ImagePath(path.to_string_lossy().into_owned()));
    }
    let _ = notification.show();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with_track(paused: bool) -> App {
        let mut app = App::with_data_dir(
            &crate::config::Config::default(),
            &std::env::temp_dir().join(format!(
                "shellmusic-system-test-{}-{:016x}",
                std::process::id(),
                crate::state_file::random_suffix()
            )),
        )
        .unwrap();
        app.queue.add(Track {
            id: "a".to_string(),
            title: "título a".to_string(),
            artist: "artista".to_string(),
            duration: Some(Duration::from_secs(200)),
            source_url: "https://example.com/a".to_string(),
            provider: ProviderKind::Youtube,
        });
        app.queue.current = Some(0);
        app.status.paused = paused;
        app
    }

    #[test]
    fn update_only_acts_on_changes() {
        // Sin controls (tests corren con media_controls apagado vía config
        // por defecto... activo, pero MediaControls sin D-Bus en CI da None):
        // el estado interno se actualiza igualmente.
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut system = SystemIntegration::new(
            &Integrations {
                notifications: false,
                media_controls: false,
                discord: false,
                discord_client_id: String::new(),
            },
            tx,
        );
        let mut app = app_with_track(false);
        system.update(&mut app);
        assert_eq!(system.last_track, Some((ProviderKind::Youtube, "a".to_string())));
        assert!(!system.last_paused);

        // Sin cambios: no debe hacer nada (last_* ya estaban así).
        system.update(&mut app);
        // Cambio de pausa: se refleja.
        app.status.paused = true;
        system.update(&mut app);
        assert!(system.last_paused);
        // Pista quitada: vuelve a None.
        app.queue.current = None;
        system.update(&mut app);
        assert_eq!(system.last_track, None);
    }
}
