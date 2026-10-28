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
        Err(err) => fatal(&format!("{err}\n\n{USAGE}"), 2),
    };
    if cli.lastfm_auth {
        return lastfm_auth().await;
    }
    if !std::io::stdout().is_terminal() {
        fatal(
            "ShellMusic necesita un terminal interactivo (TTY) para su interfaz.\n\
             Ejecuta `smusic` directamente en una terminal, sin redirigir la salida.",
            1,
        );
    }
    let ytdlp_version = match check_dependencies().await {
        Ok(version) => version,
        Err(err) => fatal(&format!("{err:#}"), 1),
    };
    let ytdlp_stale = ytdlp_staleness_warning(&ytdlp_version);
    let mut config = Config::load()?;
    if let Some(name) = &cli.theme {
        match shellmusic::theme::find_preset(name) {
            Some(preset) => config.theme = preset.clone(),
            None => {
                fatal(
                    &format!(
                        "Tema desconocido: «{name}». Disponibles: {}",
                        shellmusic::theme::preset_names().join(", ")
                    ),
                    2,
                );
            }
        }
    }
    let (player, player_events) = match MpvPlayer::spawn(config.audio.initial_volume, &config.youtube, &config.audio.format, config.audio.gapless).await {
        Ok(ok) => ok,
        Err(err) => fatal(&format!("No se pudo iniciar mpv: {err:#}"), 1),
    };
    let player = Arc::new(player);
    let providers = build_providers(&config);

    let mut terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(err) => fatal(&format!("No se pudo inicializar el terminal: {err}"), 1),
    };
    // Captura de ratón (configurable, [general] mouse): click en la barra =
    // seek, rueda = volumen. Se libera antes de restaurar el terminal.
    let mouse = config.general.mouse;
    if mouse
        && let Err(err) = crossterm::execute!(
            std::io::stdout(),
            crossterm::event::EnableMouseCapture
        )
    {
        eprintln!("No se pudo activar el ratón: {err}");
    }
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
    if mouse {
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::event::DisableMouseCapture
        );
    }
    ratatui::restore();
    match result {
        Ok(()) => Ok(()),
        Err(err) => fatal(&format!("{err:#}"), 1),
    }
}

/// Imprime `msg` en stderr y termina con `code`. En Windows, si la consola
/// la creó el SO para este proceso (doble clic sobre el .exe), espera un
/// Enter antes de salir: sin la pausa, la consola se destruye al terminar el
/// proceso y el mensaje de error queda invisible. Desde una terminal del
/// usuario no pausa (su consola no se cierra).
fn fatal(msg: &str, code: i32) -> ! {
    eprintln!("{msg}");
    pause_own_console();
    std::process::exit(code);
}

#[cfg(windows)]
fn pause_own_console() {
    if !own_console(attached_processes()) {
        return;
    }
    eprintln!("Pulsa Enter para cerrar.");
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
}

#[cfg(not(windows))]
fn pause_own_console() {}

/// Decisión pura: ¿la consola es exclusiva de este proceso? Una consola con
/// un único proceso adjunto es la que Windows crea al hacer doble clic; una
/// terminal compartida tiene siempre más (shell, proceso padre, hijos…).
#[cfg(any(windows, test))]
fn own_console(attached: u32) -> bool {
    attached == 1
}

#[cfg(windows)]
fn attached_processes() -> u32 {
    // FFI mínimo a kernel32: es la única forma de distinguir «consola creada
    // por doble clic» de «terminal del usuario». Llamada sin estado ni
    // efectos sobre un buffer propio; el unsafe queda acotado a ella.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleProcessList(process_ids: *mut u32, count: u32) -> u32;
    }
    let mut ids = [0u32; 16];
    unsafe { GetConsoleProcessList(ids.as_mut_ptr(), ids.len() as u32) }
}

const USAGE: &str = "Uso: smusic [-t <nombre>] [-p <búsqueda>] [--lastfm-auth]

Opciones:
  -t, --theme <nombre>   usa un tema empaquetado en lugar del de config.toml
  -p, --play <búsqueda>  busca al arrancar y reproduce la primera coincidencia
  --lastfm-auth          autoriza Last.fm interactivamente y guarda la sesión
  -h, --help             muestra esta ayuda";

#[derive(Debug)]
struct Cli {
    theme: Option<String>,
    play: Option<String>,
    lastfm_auth: bool,
    help: bool,
}

fn parse_args() -> std::result::Result<Cli, String> {
    parse_args_from(std::env::args().skip(1))
}

fn parse_args_from(args: impl Iterator<Item = String>) -> std::result::Result<Cli, String> {
    let mut cli = Cli {
        theme: None,
        play: None,
        lastfm_auth: false,
        help: false,
    };
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-t" | "--theme" => {
                cli.theme = Some(
                    args.next()
                        .ok_or_else(|| "-t/--theme necesita un nombre de tema".to_string())?,
                );
            }
            "-p" | "--play" => {
                cli.play = Some(
                    args.next()
                        .ok_or_else(|| "-p/--play necesita un texto de búsqueda".to_string())?,
                );
            }
            "--lastfm-auth" => cli.lastfm_auth = true,
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

/// Alta interactiva en Last.fm: pide la sesión y la guarda en el
/// directorio de datos para que el scrobbling la encuentre al arrancar.
async fn lastfm_auth() -> Result<()> {
    if !std::io::stdout().is_terminal() {
        fatal("smusic --lastfm-auth necesita un terminal interactivo.", 1);
    }
    let config = Config::load()?;
    let session = shellmusic::lastfm::auth_interactive(&config.lastfm.api_key, &config.lastfm.secret)
        .await?;
    let data_dir = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("no se pudo determinar el directorio de datos"))?
        .join("shellmusic");
    std::fs::create_dir_all(&data_dir)?;
    session.save(&data_dir)?;
    println!(
        "Sesión de Last.fm guardada para «{}». Activa el scrobbling con [lastfm] enabled = true en config.toml.",
        session.username
    );
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(tokens: &[&str]) -> std::vec::IntoIter<String> {
        tokens.iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter()
    }

    #[test]
    fn tema_largo_y_corto() {
        for flag in ["--theme", "-t"] {
            let cli = parse_args_from(args(&[flag, "dracula"])).unwrap();
            assert_eq!(cli.theme.as_deref(), Some("dracula"));
            assert!(!cli.help);
        }
    }

    #[test]
    fn play_largo_y_corto() {
        for flag in ["--play", "-p"] {
            let cli = parse_args_from(args(&[flag, "daft punk"])).unwrap();
            assert_eq!(cli.play.as_deref(), Some("daft punk"));
        }
    }

    #[test]
    fn combinados() {
        let cli = parse_args_from(args(&["-t", "thriller", "--play", "thriller"])).unwrap();
        assert_eq!(cli.theme.as_deref(), Some("thriller"));
        assert_eq!(cli.play.as_deref(), Some("thriller"));
    }

    #[test]
    fn sin_argumentos() {
        let cli = parse_args_from(args(&[])).unwrap();
        assert!(cli.theme.is_none() && cli.play.is_none() && !cli.help);
    }

    #[test]
    fn argumento_desconocido_error() {
        let err = parse_args_from(args(&["--tem"])).unwrap_err();
        assert!(err.contains("argumento desconocido"), "error: {err}");
    }

    #[test]
    fn valor_ausente_error() {
        for flag in ["--theme", "-t", "--play", "-p"] {
            let err = parse_args_from(args(&[flag])).unwrap_err();
            assert!(err.contains("necesita"), "error para {flag}: {err}");
        }
    }

    #[test]
    fn lastfm_auth_flag() {
        let cli = parse_args_from(args(&["--lastfm-auth"])).unwrap();
        assert!(cli.lastfm_auth);
        assert!(!cli.help);
        // Combinable con los demás flags.
        let cli = parse_args_from(args(&["-t", "dracula", "--lastfm-auth"])).unwrap();
        assert!(cli.lastfm_auth);
        assert_eq!(cli.theme.as_deref(), Some("dracula"));
    }

    #[test]
    fn consola_propia_solo_con_un_proceso_adjunto() {
        // Doble clic en Windows: consola dedicada con un único proceso.
        assert!(own_console(1));
        // Terminal compartida (shell, padre, hijos…) o error de la API.
        assert!(!own_console(0));
        assert!(!own_console(2));
        assert!(!own_console(17));
    }
}
