use std::io::IsTerminal;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
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
    // Los subcomandos headless hablan con la instancia en ejecución: salen
    // antes del chequeo de TTY y de dependencias (el cliente no necesita ni
    // terminal ni mpv local).
    let flags = match cli {
        Cli::Headless { cmd, query, json } => return run_headless(cmd, &query, json).await,
        Cli::Tui(flags) => flags,
    };
    if flags.lastfm_auth {
        return lastfm_auth().await;
    }
    if let Some(browser) = flags.ytm_from_browser {
        return ytm_auth_from_browser(&browser).await;
    }
    if flags.ytm_auth {
        return ytm_auth_interactive();
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
    if let Some(name) = &flags.theme {
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
            play: flags.play,
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
       smusic <comando> [búsqueda...] [--json]

Opciones:
  -t, --theme <nombre>   usa un tema empaquetado en lugar del de config.toml
  -p, --play <búsqueda>  busca al arrancar y reproduce la primera coincidencia
  --lastfm-auth          autoriza Last.fm interactivamente y guarda la sesión
  --ytm-auth             guarda los headers de YouTube Music (para [ytm])
  --from-browser[=nav]   con --ytm-auth: extrae la sesión de las cookies del
                         navegador (auto, chrome, firefox, …; pip install
                         browser-cookie3) en lugar de pegar los headers
  -h, --help             muestra esta ayuda

Comandos headless (sin TTY; requieren smusic en ejecución con [mcp] enabled):
  play <búsqueda>        reproduce la primera coincidencia
  add <búsqueda>         añade la primera coincidencia a la cola
  status                 línea compacta de lo que suena
  pause | next | prev | stop
  --json                 salida JSON cruda del socket (para scripts)";

/// Subcomando headless (RF-66): controla la instancia en ejecución por el
/// socket de control, sin abrir la TUI ni necesitar TTY.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadlessCmd {
    Play,
    Add,
    Status,
    Pause,
    Next,
    Prev,
    Stop,
}

impl HeadlessCmd {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "play" => Self::Play,
            "add" => Self::Add,
            "status" => Self::Status,
            "pause" => Self::Pause,
            "next" => Self::Next,
            "prev" => Self::Prev,
            "stop" => Self::Stop,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Play => "play",
            Self::Add => "add",
            Self::Status => "status",
            Self::Pause => "pause",
            Self::Next => "next",
            Self::Prev => "prev",
            Self::Stop => "stop",
        }
    }

    /// Los que llevan búsqueda necesitan argumentos posicionales.
    fn needs_query(self) -> bool {
        matches!(self, Self::Play | Self::Add)
    }
}

/// Flags de la modalidad TUI (arranque de la interfaz).
#[derive(Debug)]
struct TuiFlags {
    theme: Option<String>,
    play: Option<String>,
    lastfm_auth: bool,
    ytm_auth: bool,
    /// Alta de YouTube Music desde las cookies del navegador: "auto" o un
    /// nombre (chrome, firefox, …). Implica `ytm_auth`.
    ytm_from_browser: Option<String>,
}

#[derive(Debug)]
enum Cli {
    Tui(TuiFlags),
    Headless {
        cmd: HeadlessCmd,
        query: String,
        json: bool,
    },
}

fn parse_args() -> std::result::Result<Cli, String> {
    parse_args_from(std::env::args().skip(1))
}

fn parse_args_from(args: impl Iterator<Item = String>) -> std::result::Result<Cli, String> {
    let mut theme = None;
    let mut play = None;
    let mut lastfm_auth = false;
    let mut ytm_auth = false;
    let mut ytm_from_browser = None;
    let mut json = false;
    let mut cmd: Option<HeadlessCmd> = None;
    let mut query_parts: Vec<String> = Vec::new();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-t" | "--theme" => {
                theme = Some(
                    args.next()
                        .ok_or_else(|| "-t/--theme necesita un nombre de tema".to_string())?,
                );
            }
            "-p" | "--play" => {
                play = Some(
                    args.next()
                        .ok_or_else(|| "-p/--play necesita un texto de búsqueda".to_string())?,
                );
            }
            "--lastfm-auth" => lastfm_auth = true,
            "--ytm-auth" => ytm_auth = true,
            "--from-browser" => ytm_from_browser = Some("auto".to_string()),
            other if other.starts_with("--from-browser=") => {
                let name = other["--from-browser=".len()..].trim().to_lowercase();
                if name.is_empty() {
                    return Err("--from-browser necesita un nombre tras «=» o ninguno (auto)".to_string());
                }
                if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                    return Err(format!("nombre de navegador no válido: «{name}»"));
                }
                ytm_from_browser = Some(name);
            }
            "--json" => json = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other if other.starts_with('-') => {
                return Err(format!("argumento desconocido: {other}"));
            }
            other => {
                if cmd.is_none() {
                    cmd = Some(
                        HeadlessCmd::from_name(other)
                            .ok_or_else(|| format!("comando desconocido: «{other}»"))?,
                    );
                } else {
                    query_parts.push(other.to_string());
                }
            }
        }
    }
    if let Some(cmd) = cmd {
        if theme.is_some() || play.is_some() || lastfm_auth || ytm_auth || ytm_from_browser.is_some() {
            return Err(
                "los subcomandos headless no se combinan con -t/--theme, -p/--play ni --lastfm-auth/--ytm-auth/--from-browser (arrancan otro modo)"
                    .to_string(),
            );
        }
        let query = query_parts.join(" ");
        if cmd.needs_query() && query.is_empty() {
            return Err(format!("«smusic {}» necesita un texto de búsqueda", cmd.name()));
        }
        if !cmd.needs_query() && !query_parts.is_empty() {
            return Err(format!("«smusic {}» no acepta argumentos", cmd.name()));
        }
        return Ok(Cli::Headless { cmd, query, json });
    }
    if json {
        return Err(
            "«--json» solo aplica a los subcomandos headless (play, add, status, pause, next, prev, stop)"
                .to_string(),
        );
    }
    // --from-browser implica el alta de YouTube Music.
    if ytm_from_browser.is_some() {
        ytm_auth = true;
    }
    Ok(Cli::Tui(TuiFlags {
        theme,
        play,
        lastfm_auth,
        ytm_auth,
        ytm_from_browser,
    }))
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

/// Alta interactiva en YouTube Music (RF-68): pide los headers del navegador
/// en cualquiera de los formatos que copian las herramientas de desarrollo
/// (vista cruda «Nombre: valor», «Copiar como cURL», «Copiar como fetch» o
/// JSON) y los guarda en el data dir con permisos 0600 para el puente
/// ytmusicapi. Con que incluyan la cookie de sesión basta: el puente completa
/// origin, user-agent y el SAPISIDHASH.
fn ytm_auth_interactive() -> Result<()> {
    use std::io::BufRead;
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        fatal("smusic --ytm-auth necesita un terminal interactivo.", 1);
    }
    let data_dir = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("no se pudo determinar el directorio de datos"))?
        .join("shellmusic");
    std::fs::create_dir_all(&data_dir)?;
    let path = shellmusic::ytm::headers_path(&data_dir);
    if path.exists() {
        println!("Ya hay headers guardados; pegando de nuevo los reemplaza.\n");
    }
    println!("Alta en YouTube Music");
    println!("  1. Abre https://music.youtube.com con tu cuenta.");
    println!("  2. F12 → Red (Network) → recarga la página.");
    println!("  3. Clic derecho en la petición «browse» → «Copiar como cURL» (también vale");
    println!("     «Copiar como fetch» o la vista cruda de «Request Headers»).");
    println!("  4. Pégalo aquí abajo (lo único imprescindible es la línea cookie:) y pulsa Enter dos veces.");
    println!();
    let mut text = String::new();
    let stdin = std::io::stdin();
    let mut saw_blank = false;
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            // Una línea en blanco termina tras contenido; dos seguidas, también.
            if saw_blank || !text.trim().is_empty() {
                break;
            }
            saw_blank = true;
            continue;
        }
        saw_blank = false;
        text.push_str(&line);
        text.push('\n');
    }
    // Validación mínima: debe llevar la cookie de la sesión.
    if !text.to_lowercase().contains("cookie") {
        fatal(
            "no se detectó una cookie en lo pegado; copia las cabeceras completas de la petición",
            2,
        );
    }
    std::fs::write(&path, text.trim_end()).with_context(|| format!("no se pudo escribir {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    println!("Headers guardados en {}.", path.display());
    println!("Ahora: pip install ytmusicapi y [ytm] enabled = true en config.toml.");
    Ok(())
}

/// Alta en YouTube Music sin DevTools (RF-68): extrae la sesión de las
/// cookies del navegador vía browser_cookie3 y guarda los headers resultantes
/// (JSON, 0600). `browser` es "auto" o un nombre (chrome, firefox, …).
async fn ytm_auth_from_browser(browser: &str) -> Result<()> {
    let data_dir = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("no se pudo determinar el directorio de datos"))?
        .join("shellmusic");
    std::fs::create_dir_all(&data_dir)?;
    let headers = match shellmusic::ytm::auth_from_browser(&data_dir, browser).await {
        Ok(headers) => headers,
        Err(err) => fatal(&format!("{err:#}"), 1),
    };
    let path = shellmusic::ytm::headers_path(&data_dir);
    let text = serde_json::to_string_pretty(&headers)
        .context("no se pudieron serializar los headers extraídos")?;
    std::fs::write(&path, text).with_context(|| format!("no se pudo escribir {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    println!("Sesión de YouTube Music extraída del navegador y guardada en {}.", path.display());
    println!("Ahora: pip install ytmusicapi y [ytm] enabled = true en config.toml.");
    Ok(())
}

/// Timeout de las peticiones headless que implican búsqueda: resolver y
/// cargar la primera coincidencia puede llevar varios segundos.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);
/// Timeout de las peticiones headless de control directo.
const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);

/// Ejecuta un subcomando headless (RF-66) contra la instancia en ejecución:
/// traduce a una petición del socket de control, imprime la respuesta y sale
/// con código 0 (ok) o 1 (error de la app o de conexión).
async fn run_headless(cmd: HeadlessCmd, query: &str, json: bool) -> Result<()> {
    use serde_json::{Value, json};
    let body = match cmd {
        HeadlessCmd::Play => json!({ "req": "play", "query": query }),
        HeadlessCmd::Add => json!({ "req": "queue_add", "query": query }),
        HeadlessCmd::Status => json!({ "req": "now_playing" }),
        HeadlessCmd::Pause => json!({ "req": "control", "command": "pause" }),
        HeadlessCmd::Next => json!({ "req": "control", "command": "next" }),
        HeadlessCmd::Prev => json!({ "req": "control", "command": "prev" }),
        HeadlessCmd::Stop => json!({ "req": "control", "command": "stop" }),
    };
    let timeout = if cmd.needs_query() {
        SEARCH_TIMEOUT
    } else {
        CONTROL_TIMEOUT
    };
    let response = match tokio::time::timeout(timeout, shellmusic::control::request(body)).await {
        Ok(Ok(response)) => response,
        // El error ya explica la causa accionable (p. ej. activar [mcp]).
        Ok(Err(err)) => fatal(&format!("{err:#}"), 1),
        Err(_) => fatal(
            "tiempo de espera agotado esperando respuesta de la instancia de smusic",
            1,
        ),
    };
    let ok = response.get("ok").and_then(Value::as_bool) == Some(true);
    if json {
        println!("{}", serde_json::to_string(&response).unwrap_or_default());
    } else if ok {
        println!("{}", human_line(cmd, &response));
    }
    if !ok {
        let error = response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("error desconocido de la instancia");
        fatal(error, 1);
    }
    Ok(())
}

/// Salida legible de un subcomando headless (con `--json` va el JSON crudo).
fn human_line(cmd: HeadlessCmd, response: &serde_json::Value) -> String {
    let data = &response["data"];
    match cmd {
        HeadlessCmd::Status => status_line(data),
        HeadlessCmd::Play => track_line("♪ sonando", &data["playing"]),
        HeadlessCmd::Add => track_line("+ en cola", &data["queued"]),
        HeadlessCmd::Pause | HeadlessCmd::Next | HeadlessCmd::Prev | HeadlessCmd::Stop => data
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("hecho")
            .to_string(),
    }
}

/// «♪ Título — Artista (m:ss/m:ss)»; «⏸ …» en pausa; «nada sonando» si la
/// cola está vacía.
fn status_line(data: &serde_json::Value) -> String {
    use serde_json::Value;
    let Some(track) = data.get("track").filter(|t| !t.is_null()) else {
        return "nada sonando".to_string();
    };
    let title = track.get("title").and_then(Value::as_str).unwrap_or("?");
    let artist = track.get("artist").and_then(Value::as_str).unwrap_or("?");
    let clock = match (
        data.get("position_secs").and_then(Value::as_u64),
        data.get("duration_secs").and_then(Value::as_u64),
    ) {
        (Some(pos), Some(dur)) => format!(" ({}/{})", fmt_clock(pos), fmt_clock(dur)),
        _ => String::new(),
    };
    let state = if data.get("paused").and_then(Value::as_bool) == Some(true) {
        "⏸ "
    } else {
        "♪ "
    };
    format!("{state}{title} — {artist}{clock}")
}

fn track_line(prefix: &str, track: &serde_json::Value) -> String {
    let title = track
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("?");
    let artist = track
        .get("artist")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("?");
    format!("{prefix}: {title} — {artist}")
}

fn fmt_clock(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
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

    /// Parsea en modo TUI; falla el test si el parseo dio headless.
    fn tui(tokens: &[&str]) -> TuiFlags {
        match parse_args_from(args(tokens)).unwrap() {
            Cli::Tui(flags) => flags,
            Cli::Headless { .. } => panic!("se esperaba modo TUI para {tokens:?}"),
        }
    }

    #[test]
    fn tema_largo_y_corto() {
        for flag in ["--theme", "-t"] {
            let flags = tui(&[flag, "dracula"]);
            assert_eq!(flags.theme.as_deref(), Some("dracula"));
        }
    }

    #[test]
    fn play_largo_y_corto() {
        for flag in ["--play", "-p"] {
            let flags = tui(&[flag, "daft punk"]);
            assert_eq!(flags.play.as_deref(), Some("daft punk"));
        }
    }

    #[test]
    fn combinados() {
        let flags = tui(&["-t", "thriller", "--play", "thriller"]);
        assert_eq!(flags.theme.as_deref(), Some("thriller"));
        assert_eq!(flags.play.as_deref(), Some("thriller"));
    }

    #[test]
    fn sin_argumentos() {
        let flags = tui(&[]);
        assert!(flags.theme.is_none() && flags.play.is_none() && !flags.lastfm_auth);
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
        let flags = tui(&["--lastfm-auth"]);
        assert!(flags.lastfm_auth);
        assert!(!flags.ytm_auth);
        // Combinable con los demás flags.
        let flags = tui(&["-t", "dracula", "--lastfm-auth"]);
        assert!(flags.lastfm_auth);
        assert_eq!(flags.theme.as_deref(), Some("dracula"));
    }

    #[test]
    fn ytm_auth_flag() {
        let flags = tui(&["--ytm-auth"]);
        assert!(flags.ytm_auth);
        assert!(!flags.lastfm_auth);
        let flags = tui(&["--ytm-auth", "-t", "dracula"]);
        assert!(flags.ytm_auth);
        assert_eq!(flags.theme.as_deref(), Some("dracula"));
    }

    #[test]
    fn ytm_auth_from_browser_flag() {
        // Sin nombre: auto-detección; implica el alta de YouTube Music.
        let flags = tui(&["--ytm-auth", "--from-browser"]);
        assert!(flags.ytm_auth);
        assert_eq!(flags.ytm_from_browser.as_deref(), Some("auto"));
        // Con nombre (se normaliza a minúsculas); no hace falta --ytm-auth.
        let flags = tui(&["--from-browser=FireFox"]);
        assert!(flags.ytm_auth);
        assert_eq!(flags.ytm_from_browser.as_deref(), Some("firefox"));
        // Errores de valor.
        let err = parse_args_from(args(&["--from-browser="])).unwrap_err();
        assert!(err.contains("necesita un nombre"), "error: {err}");
        let err = parse_args_from(args(&["--from-browser=fox fire!"])).unwrap_err();
        assert!(err.contains("no válido"), "error: {err}");
        let err = parse_args_from(args(&["--from-browsers"])).unwrap_err();
        assert!(err.contains("argumento desconocido"), "error: {err}");
        // No combina con los subcomandos headless.
        let err = parse_args_from(args(&["--from-browser", "play", "x"])).unwrap_err();
        assert!(err.contains("no se combinan"), "error: {err}");
    }

    #[test]
    fn headless_play_junta_la_busqueda() {
        let Cli::Headless { cmd, query, json } =
            parse_args_from(args(&["play", "daft", "punk"])).unwrap()
        else {
            panic!("se esperaba modo headless");
        };
        assert_eq!(cmd, HeadlessCmd::Play);
        assert_eq!(query, "daft punk");
        assert!(!json);
    }

    #[test]
    fn headless_json_en_cualquier_posicion() {
        for tokens in [&["add", "daft", "--json"][..], &["--json", "add", "daft"][..]] {
            let Cli::Headless { cmd, query, json } =
                parse_args_from(args(tokens)).unwrap()
            else {
                panic!("se esperaba modo headless para {tokens:?}");
            };
            assert_eq!(cmd, HeadlessCmd::Add);
            assert_eq!(query, "daft");
            assert!(json);
        }
    }

    #[test]
    fn headless_status_y_control_no_llevan_busqueda() {
        for sub in ["status", "pause", "next", "prev", "stop"] {
            let Cli::Headless { cmd, query, .. } =
                parse_args_from(args(&[sub])).unwrap()
            else {
                panic!("se esperaba modo headless para {sub}");
            };
            assert_eq!(cmd, HeadlessCmd::from_name(sub).unwrap());
            assert!(query.is_empty());
        }
    }

    #[test]
    fn headless_control_rechaza_argumentos_extra() {
        for sub in ["status", "pause", "next", "prev", "stop"] {
            let err = parse_args_from(args(&[sub, "extra"])).unwrap_err();
            assert!(err.contains("no acepta argumentos"), "error para {sub}: {err}");
        }
    }

    #[test]
    fn headless_play_sin_busqueda_error() {
        for sub in ["play", "add"] {
            let err = parse_args_from(args(&[sub])).unwrap_err();
            assert!(
                err.contains("necesita un texto de búsqueda"),
                "error para {sub}: {err}"
            );
        }
    }

    #[test]
    fn headless_no_combina_con_flags_de_tui() {
        let err = parse_args_from(args(&["-p", "x", "play", "y"])).unwrap_err();
        assert!(err.contains("no se combinan"), "error: {err}");
    }

    #[test]
    fn json_sin_subcomando_error() {
        let err = parse_args_from(args(&["--json"])).unwrap_err();
        assert!(err.contains("solo aplica"), "error: {err}");
    }

    #[test]
    fn comando_desconocido_error() {
        let err = parse_args_from(args(&["ply", "daft"])).unwrap_err();
        assert!(err.contains("comando desconocido"), "error: {err}");
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
