use std::io::IsTerminal;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use shellmusic::app;
use shellmusic::config::Config;
use shellmusic::player::mpv::MpvPlayer;
use shellmusic::providers::cache::CachingProvider;
use shellmusic::providers::bandcamp::BandcampProvider;
use shellmusic::providers::soundcloud::SoundcloudProvider;
use shellmusic::providers::youtube::YoutubeProvider;
use shellmusic::providers::{MusicProvider, ProviderKind};
use tokio::process::Command;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = match parse_args() {
        Ok(cli) => cli,
        Err(err) => {
            eprintln!("{err}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    if !std::io::stdout().is_terminal() {
        eprintln!(
            "ShellMusic necesita un terminal interactivo (TTY) para su interfaz.\n\
             Ejecuta `smusic` directamente en una terminal, sin redirigir la salida."
        );
        std::process::exit(1);
    }
    let ytdlp_version = match check_dependencies().await {
        Ok(version) => version,
        Err(err) => {
            eprintln!("{err:#}");
            std::process::exit(1);
        }
    };
    let ytdlp_stale = ytdlp_staleness_warning(&ytdlp_version);
    let mut config = Config::load()?;
    if let Some(name) = &cli.theme {
        match shellmusic::theme::find_preset(name) {
            Some(preset) => config.theme = preset.clone(),
            None => {
                eprintln!(
                    "Tema desconocido: «{name}». Disponibles: {}",
                    shellmusic::theme::preset_names().join(", ")
                );
                std::process::exit(2);
            }
        }
    }
    let (player, player_events) = match MpvPlayer::spawn(config.audio.initial_volume, &config.youtube, &config.audio.format).await {
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
    // La detección del protocolo de imagen consulta al terminal por
    // stdin/stdout: debe hacerse antes de arrancar el lector de eventos.
    // Los terminales que responden lo hacen en milisegundos; con los que
    // no (screen, linux, emuladores mudos) el timeout por defecto (2 s por
    // consulta) retrasaba varios segundos el primer frame. Un timeout corto
    // cae a halfblocks sin castigar a los terminales reales.
    let picker = ratatui_image::picker::Picker::from_query_stdio_with_options(
        ratatui_image::picker::cap_parser::QueryStdioOptions {
            timeout: Duration::from_millis(500),
            ..Default::default()
        },
    )
    .unwrap_or_else(|_| ratatui_image::picker::Picker::halfblocks());
    let result = app::run(
        &mut terminal,
        providers,
        player,
        player_events,
        config,
        picker,
        app::Startup {
            warning: ytdlp_stale,
            play: cli.play,
        },
    )
    .await;
    ratatui::restore();
    result
}

const USAGE: &str = "Uso: smusic [--theme <nombre>] [--play <búsqueda>]

Opciones:
  --theme <nombre>   usa un tema empaquetado en lugar del de config.toml
  --play <búsqueda>  busca al arrancar y reproduce la primera coincidencia
  -h, --help         muestra esta ayuda";

struct Cli {
    theme: Option<String>,
    play: Option<String>,
    help: bool,
}

fn parse_args() -> std::result::Result<Cli, String> {
    let mut cli = Cli {
        theme: None,
        play: None,
        help: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--theme" => {
                cli.theme = Some(
                    args.next()
                        .ok_or_else(|| "--theme necesita un nombre de tema".to_string())?,
                );
            }
            "--play" => {
                cli.play = Some(
                    args.next()
                        .ok_or_else(|| "--play necesita un texto de búsqueda".to_string())?,
                );
            }
            "-h" | "--help" => cli.help = true,
            other => return Err(format!("argumento desconocido: {other}")),
        }
    }
    if cli.help {
        println!("{USAGE}");
        std::process::exit(0);
    }
    Ok(cli)
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
                ProviderKind::Youtube => Arc::new(YoutubeProvider::new(&config.youtube)),
                ProviderKind::Soundcloud => Arc::new(SoundcloudProvider::new()),
                ProviderKind::Bandcamp => Arc::new(BandcampProvider::new()),
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

async fn check_dependencies() -> Result<String> {
    let (mpv, ytdlp) = tokio::join!(
        check(
            "mpv",
            "Linux: sudo apt install mpv · macOS: brew install mpv · Windows: winget install mpv",
        ),
        check(
            "yt-dlp",
            "Linux/macOS: pip install yt-dlp (o tu gestor de paquetes) · Windows: winget install yt-dlp",
        ),
    );
    mpv?;
    // La versión de yt-dlp se reutiliza para el aviso de antigüedad: evita
    // lanzar el proceso por tercera vez (~0,3 s de arranque de Python).
    ytdlp
}

/// Comprueba que `binary` está en el PATH y devuelve la primera línea de
/// `{binary} --version`; si falta, balea con instrucciones de instalación.
async fn check(binary: &str, hint: &str) -> Result<String> {
    let output = Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await;
    match output {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
        _ => bail!("No se encontró «{binary}» en el PATH.\nInstálalo con: {hint}"),
    }
}

/// Los parches anti-bot de YouTube llegan vía releases de yt-dlp: una versión
/// de más de 90 días es la causa más común de roturas. Devuelve un aviso
/// para mostrar en la TUI al arrancar (no aborta el arranque).
fn ytdlp_staleness_warning(version: &str) -> Option<String> {
    let days = shellmusic::ytdlp_version_age_days(version)?;
    if days > 90 {
        Some(format!(
            "yt-dlp es de {version} ({days} días); si YouTube falla, actualízalo: pip install -U yt-dlp"
        ))
    } else {
        None
    }
}
