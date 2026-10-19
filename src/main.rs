use std::io::IsTerminal;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::{Result, bail};
use shellmusic::app;
use shellmusic::config::Config;
use shellmusic::player::mpv::MpvPlayer;
use shellmusic::providers::youtube::YoutubeProvider;
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
    let provider = Arc::new(YoutubeProvider::new());

    let mut terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(err) => {
            eprintln!("No se pudo inicializar el terminal: {err}");
            std::process::exit(1);
        }
    };
    let result = app::run(&mut terminal, provider, player, player_events, config).await;
    ratatui::restore();
    result
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
