pub mod mpv;

use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;

#[derive(Debug, Clone, Default)]
pub struct PlaybackStatus {
    pub position: Option<Duration>,
    pub duration: Option<Duration>,
    pub paused: bool,
    pub volume: u8,
}

#[derive(Debug, Clone)]
pub struct PlayerNotice {
    pub text: String,
    pub is_error: bool,
}

impl PlayerNotice {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: false,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: true,
        }
    }
}

#[derive(Debug, Clone)]
pub enum PlayerEvent {
    Notice(PlayerNotice),
    /// La pista terminó de forma natural (end-file con reason "eof").
    TrackEnded,
}

#[async_trait]
pub trait Player: Send + Sync {
    async fn play(&self, url: &str) -> Result<()>;
    async fn toggle_pause(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    async fn set_volume(&self, volume: u8) -> Result<()>;
    async fn status(&self) -> Result<PlaybackStatus>;
}
