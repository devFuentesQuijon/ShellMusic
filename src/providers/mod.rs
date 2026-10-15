pub mod youtube;

use std::fmt;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderKind {
    Youtube,
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderKind::Youtube => write!(f, "YouTube"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration: Option<Duration>,
    pub source_url: String,
    pub provider: ProviderKind,
}

#[async_trait]
pub trait MusicProvider: Send + Sync {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>>;
}
