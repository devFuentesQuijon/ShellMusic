use std::io::IsTerminal;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use shellmusic::app;
use shellmusic::config::Config;
use shellmusic::player::mpv::MpvPlayer;
use shellmusic::providers::cache::CachingProvider;
use shellmusic::providers::soundcloud::SoundcloudProvider;
use shellmusic::providers::youtube::YoutubeProvider;
use shellmusic::providers::{MusicProvider, ProviderKind};
use tokio::process::Command;

#[tokio::main]
async fn main() -> Result<()> {
    if !std::io::stdout().is_terminal() {
        eprintln!(
            "ShellMusic necesita un terminal interactivo (TTY) para su interfaz.\n\
             Ejecuta `smusic` directamente en una terminal, sin redirigir la salida."
        );
        std::process::exit(1);
    }
    if let Err(err) = check_dependencies().await {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
    let config = Config::load()?;
    let (player, player_events) = match MpvPlayer::spawn(config.audio.initial_volume, &config.youtube).await {
        Ok(ok) => ok,
        Err(err) => {
            eprintln!("No se pudo iniciar mpv: {err:#}");
            std::process::exit(1);
        }
    };
    let player = Arc::new(player);
    let providers = build_providers(&config);

    let mut terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(err) => {
            eprintln!("No se pudo inicializar el terminal: {err}");
            std::process::exit(1);
        }
    };
    let result = app::run(&mut terminal, providers, player, player_events, config).await;
    ratatui::restore();
    result
}

fn build_providers(config: &Config) -> Vec<(ProviderKind, Arc<dyn MusicProvider>)> {
    let mut kinds: Vec<ProviderKind> = config
        .general
        .providers
        .iter()
        .filter_map(|name| ProviderKind::from_name(name))
        .collect();
    if let Some(default) = ProviderKind::from_name(&config.general.default_provider)
        && let Some(pos) = kinds.iter().position(|kind| *kind == default)
    {
        kinds.swap(0, pos);
    }
    kinds
        .into_iter()
        .map(|kind| {
            let provider: Arc<dyn MusicProvider> = match kind {
                ProviderKind::Youtube => Arc::new(YoutubeProvider::new()),
                ProviderKind::Soundcloud => Arc::new(SoundcloudProvider::new()),
            };
            let provider = if config.cache.enabled {
                Arc::new(CachingProvider::new(
                    provider,
                    kind,
                    Duration::from_secs(config.cache.ttl_minutes * 60),
                )) as Arc<dyn MusicProvider>
            } else {
                provider
            };
            (kind, provider)
        })
        .collect()
}

async fn check_dependencies() -> Result<()> {
    check(
        "mpv",
        "Linux: sudo apt install mpv · macOS: brew install mpv · Windows: winget install mpv",
    )
    .await?;
    check(
        "yt-dlp",
        "Linux/macOS: pip install yt-dlp (o tu gestor de paquetes) · Windows: winget install yt-dlp",
    )
    .await?;
    Ok(())
}

async fn check(binary: &str, hint: &str) -> Result<()> {
    let available = Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false);
    if !available {
        bail!("No se encontró «{binary}» en el PATH.\nInstálalo con: {hint}");
    }
    Ok(())
}
