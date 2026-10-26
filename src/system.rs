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
        Self {
            notifications: config.notifications,
            controls,
            last_track: None,
            last_paused: false,
            startup_warning: warning,
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn new(
        config: &Integrations,
        _tx: mpsc::UnboundedSender<MediaCommand>,
    ) -> Self {
        Self {
            notifications: config.notifications,
            last_track: None,
            last_paused: false,
            startup_warning: None,
        }
    }

    /// Publica metadata/notificación solo cuando cambia la pista o la pausa.
    pub fn update(&mut self, app: &App) {
        let track = app.queue.current_track();
        let key = track.map(|t| (t.provider, t.id.clone()));
        let paused = app.status.paused;
        if key == self.last_track && paused == self.last_paused {
            return;
        }
        let track_changed = key != self.last_track;
        self.last_track = key;
        self.last_paused = paused;

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
            notify(track);
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

fn notify(track: &Track) {
    // El fallo de una notificación no debe ensuciar la UI: se ignora.
    let _ = notify_rust::Notification::new()
        .appname("ShellMusic")
        .summary(&track.title)
        .body(&track.artist)
        .show();
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
            },
            tx,
        );
        let mut app = app_with_track(false);
        system.update(&app);
        assert_eq!(system.last_track, Some((ProviderKind::Youtube, "a".to_string())));
        assert!(!system.last_paused);

        // Sin cambios: no debe hacer nada (last_* ya estaban así).
        system.update(&app);
        // Cambio de pausa: se refleja.
        app.status.paused = true;
        system.update(&app);
        assert!(system.last_paused);
        // Pista quitada: vuelve a None.
        app.queue.current = None;
        system.update(&app);
        assert_eq!(system.last_track, None);
    }
}
