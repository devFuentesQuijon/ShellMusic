use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};

use super::{PlaybackStatus, Player, PlayerEvent, PlayerNotice};
use crate::browsers;
use crate::config::Youtube;

const IPC_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const PLAY_TIMEOUT: Duration = Duration::from_secs(45);
const STDERR_TAIL_CAP: usize = 20;

#[cfg(unix)]
type IpcStream = tokio::net::UnixStream;
#[cfg(windows)]
type IpcStream = tokio::net::windows::named_pipe::NamedPipeClient;

#[cfg(unix)]
fn ipc_server_path() -> PathBuf {
    let instance = INSTANCE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir()
        .join(format!(
            "shellmusic-mpv-{}-{instance}-{:016x}",
            std::process::id(),
            crate::state_file::random_suffix()
        ))
        .join("mpv.sock")
}

#[cfg(windows)]
fn ipc_server_path() -> PathBuf {
    let instance = INSTANCE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    PathBuf::from(format!(
        r"\\.\pipe\shellmusic-mpv-{}-{instance}-{:016x}",
        std::process::id(),
        crate::state_file::random_suffix()
    ))
}

static INSTANCE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(unix)]
async fn ipc_connect(path: &Path) -> Result<IpcStream> {
    let deadline = Instant::now() + IPC_CONNECT_TIMEOUT;
    loop {
        match tokio::net::UnixStream::connect(path).await {
            Ok(stream) => return Ok(stream),
            Err(err) if Instant::now() < deadline => {
                let _ = err;
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(err) => {
                return Err(err).context("no se pudo conectar al socket IPC de mpv");
            }
        }
    }
}

#[cfg(windows)]
async fn ipc_connect(path: &Path) -> Result<IpcStream> {
    let deadline = Instant::now() + IPC_CONNECT_TIMEOUT;
    loop {
        match tokio::net::windows::named_pipe::ClientOptions::new().open(path) {
            Ok(client) => return Ok(client),
            Err(err) if Instant::now() < deadline => {
                let _ = err;
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(err) => {
                return Err(err).context("no se pudo conectar al named pipe IPC de mpv");
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CookiesMode {
    Auto,
    Disabled,
    Fixed,
}

impl CookiesMode {
    fn from_config(value: &str) -> Self {
        if value.eq_ignore_ascii_case("auto") {
            Self::Auto
        } else if value.is_empty() {
            Self::Disabled
        } else {
            Self::Fixed
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CookieAttempt {
    None,
    Browser(String),
}

/// Un intento de reproducción: cliente de YouTube emulado × cookies.
/// YouTube bloquea clientes distintos según la IP y la época, así que ante
/// un bloqueo se rota la combinación completa.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PlayAttempt {
    /// Cliente para `youtube:player_client`; "" = el por defecto de yt-dlp.
    client: String,
    cookies: CookieAttempt,
}

/// Cadena de clientes de fallback cuando el configurado falla.
const CLIENT_FALLBACKS: &[&str] = &["android", "ios", "tv", "web_embedded"];

/// Construye el plan de intentos cliente × cookies: con PO Token va primero
/// el cliente por defecto de yt-dlp, luego el configurado, luego los
/// fallbacks, sin duplicados.
fn build_attempt_plan(
    configured_client: &str,
    pot_available: bool,
    cookie_attempts: &[CookieAttempt],
) -> Vec<PlayAttempt> {
    let mut clients: Vec<String> = Vec::new();
    if pot_available {
        clients.push(String::new());
    }
    clients.push(configured_client.to_string());
    clients.extend(CLIENT_FALLBACKS.iter().map(|c| c.to_string()));
    let mut seen = std::collections::HashSet::new();
    clients.retain(|c| seen.insert(c.clone()));
    clients
        .into_iter()
        .flat_map(|client| {
            cookie_attempts
                .iter()
                .cloned()
                .map(|cookies| PlayAttempt {
                    client: client.clone(),
                    cookies,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

impl CookieAttempt {
    fn label(&self) -> &str {
        match self {
            Self::None => "sin cookies",
            Self::Browser(browser) => browser,
        }
    }
}

impl PlayAttempt {
    fn label(&self) -> String {
        let client = if self.client.is_empty() {
            "default"
        } else {
            self.client.as_str()
        };
        format!("cliente {client}, {}", self.cookies.label())
    }
}

#[derive(Default)]
struct Shared {
    position: Option<f64>,
    duration: Option<f64>,
    paused: bool,
    volume: i64,
    pending: HashMap<u64, oneshot::Sender<Value>>,
    next_id: u64,
    stderr_tail: VecDeque<String>,
    outcome_tx: Option<oneshot::Sender<Result<(), String>>>,
    /// Entrada de playlist del `loadfile` en vuelo: los `end-file` de la
    /// entrada anterior (p. ej. un `stop` previo) no deben resolver la
    /// espera de la nueva carga.
    pending_entry: Option<i64>,
}

pub struct MpvPlayer {
    child: Child,
    outgoing: mpsc::UnboundedSender<Value>,
    notify: mpsc::UnboundedSender<PlayerEvent>,
    shared: Arc<Mutex<Shared>>,
    socket_path: PathBuf,
    youtube: Youtube,
    cookies_mode: CookiesMode,
    /// Combinación cliente × cookies que funcionó en esta sesión.
    cached_attempt: Mutex<Option<PlayAttempt>>,
    detected_browsers: Mutex<Option<Vec<String>>>,
    /// Detección cacheada del plugin bgutil PO Token de yt-dlp.
    pot_detected: Mutex<Option<bool>>,
}

impl MpvPlayer {
    pub async fn spawn(
        initial_volume: u8,
        youtube: &Youtube,
    ) -> Result<(Self, mpsc::UnboundedReceiver<PlayerEvent>)> {
        Self::spawn_with_args(initial_volume, youtube, &[]).await
    }

    pub async fn spawn_with_args(
        initial_volume: u8,
        youtube: &Youtube,
        extra_args: &[&str],
    ) -> Result<(Self, mpsc::UnboundedReceiver<PlayerEvent>)> {
        let socket_path = ipc_server_path();
        // El socket vive en un directorio privado 0700 para que otro usuario
        // local no pueda suplantarlo y capturar el canal IPC.
        #[cfg(unix)]
        if let Some(dir) = socket_path.parent() {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)
                .context("no se pudo crear el directorio del socket IPC")?;
        }

        let mut command = Command::new("mpv");
        command.kill_on_drop(true);
        command
            .arg("--idle=yes")
            .arg("--no-video")
            .arg("--msg-level=all=warn")
            .arg(format!("--input-ipc-server={}", socket_path.display()))
            .arg(format!("--volume={}", initial_volume.min(100)))
            .args(extra_args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = command.spawn().context("no se pudo lanzar mpv")?;
        let stderr = child.stderr.take();

        let stream = ipc_connect(&socket_path).await?;
        let (read_half, write_half) = tokio::io::split(stream);

        let shared = Arc::new(Mutex::new(Shared {
            volume: i64::from(initial_volume.min(100)),
            ..Shared::default()
        }));
        if let Some(stderr) = stderr {
            tokio::spawn(stderr_task(stderr, Arc::clone(&shared)));
        }
        let (out_tx, out_rx) = mpsc::unbounded_channel();
        let (notify_tx, notify_rx) = mpsc::unbounded_channel();
        tokio::spawn(writer_task(write_half, out_rx));
        tokio::spawn(reader_task(
            read_half,
            Arc::clone(&shared),
            notify_tx.clone(),
        ));

        let player = Self {
            child,
            outgoing: out_tx,
            notify: notify_tx,
            shared,
            socket_path,
            cookies_mode: CookiesMode::from_config(&youtube.cookies_from_browser),
            youtube: youtube.clone(),
            cached_attempt: Mutex::new(None),
            detected_browsers: Mutex::new(None),
            pot_detected: Mutex::new(None),
        };
        for (id, name) in [
            (1, "time-pos"),
            (2, "duration"),
            (3, "pause"),
            (4, "volume"),
        ] {
            player
                .command(vec!["observe_property".into(), id.into(), name.into()])
                .await?;
        }
        Ok((player, notify_rx))
    }

    async fn command(&self, args: Vec<Value>) -> Result<Value> {
        let (id, rx) = {
            let mut shared = lock(&self.shared);
            let id = shared.next_id;
            shared.next_id += 1;
            let (tx, rx) = oneshot::channel();
            shared.pending.insert(id, tx);
            (id, rx)
        };
        self.outgoing
            .send(json!({ "command": args, "request_id": id }))
            .context("el canal IPC de mpv está cerrado")?;
        let response = tokio::time::timeout(COMMAND_TIMEOUT, rx)
            .await
            .context("mpv no respondió a tiempo")?
            .context("mpv cerró la conexión IPC")?;
        let error = response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        if error != "success" {
            bail!("mpv: {error}");
        }
        Ok(response)
    }

    /// El plugin bgutil PO Token de yt-dlp (genera PO Tokens locales, la
    /// defensa actual contra el 403 de GVS). Detección cacheada: paquete pip
    /// o carpeta de plugin de yt-dlp.
    fn pot_provider_available(&self) -> bool {
        if self.youtube.pot_provider == "off" {
            return false;
        }
        lock(&self.pot_detected)
            .get_or_insert_with(detect_pot_provider)
            .to_owned()
    }

    /// Plan de intentos: la combinación cliente × cookies que funcionó en
    /// esta sesión va primero (y sola, mientras siga funcionando); si no hay
    /// caché, se recorre la cadena de clientes —el configurado primero— y
    /// para cada uno las cookies según el modo configurado. Con el plugin PO
    /// Token disponible se prueba primero el cliente por defecto de yt-dlp
    /// (el plugin suele dar mejores streams que forzar `android`).
    fn attempt_plan(&self) -> Vec<PlayAttempt> {
        if let Some(cached) = lock(&self.cached_attempt).clone() {
            return vec![cached];
        }
        let cookie_attempts: Vec<CookieAttempt> = match self.cookies_mode {
            CookiesMode::Disabled => vec![CookieAttempt::None],
            CookiesMode::Fixed => vec![CookieAttempt::Browser(
                self.youtube.cookies_from_browser.clone(),
            )],
            CookiesMode::Auto => {
                let mut attempts = vec![CookieAttempt::None];
                attempts.extend(
                    self.installed_browsers().into_iter().map(CookieAttempt::Browser),
                );
                attempts
            }
        };
        build_attempt_plan(
            &self.youtube.player_client,
            self.pot_provider_available(),
            &cookie_attempts,
        )
    }

    fn installed_browsers(&self) -> Vec<String> {
        let mut detected = lock(&self.detected_browsers);
        detected
            .get_or_insert_with(|| {
                browsers::detect_installed()
                    .into_iter()
                    .map(str::to_string)
                    .collect()
            })
            .clone()
    }

    async fn apply_attempt(&self, attempt: &PlayAttempt) -> Result<()> {
        let options = ytdl_raw_options(&self.youtube, attempt).unwrap_or_default();
        self.command(vec![
            "set_property".into(),
            "ytdl-raw-options".into(),
            options.into(),
        ])
        .await?;
        Ok(())
    }

    async fn try_play(&self, url: &str, start: Option<Duration>) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        {
            let mut shared = lock(&self.shared);
            shared.stderr_tail.clear();
            shared.outcome_tx = Some(tx);
            // Centinela: la espera queda armada pero aún no se vio el
            // start-file de esta carga; un end-file rezagado de la entrada
            // anterior (p. ej. de un stop previo) no debe resolverla.
            shared.pending_entry = Some(-1);
        }
        // mpv 0.37 no admite opciones por archivo en `loadfile` (llegaron en
        // 0.38), así que la reanudación de sesión se hace con un seek
        // absoluto en cuanto arranca la reproducción.
        self.command(vec!["loadfile".into(), url.into()])
            .await
            .map_err(|err| err.to_string())?;
        let result = match tokio::time::timeout(PLAY_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            // El emisor se descartó: otro play() tomó el relevo.
            Ok(Err(_)) => Ok(()),
            Err(_) => {
                let mut shared = lock(&self.shared);
                shared.outcome_tx = None;
                shared.pending_entry = None;
                Err("tiempo de espera agotado al cargar el stream".to_string())
            }
        };
        if result.is_ok()
            && let Some(start) = start
            && start > Duration::ZERO
            && let Err(err) = self
                .command(vec![
                    "set_property".into(),
                    "time-pos".into(),
                    start.as_secs_f64().into(),
                ])
                .await
        {
            // La pista ya suena: un fallo al reanudar la posición solo se avisa.
            let _ = self.notify.send(PlayerEvent::Notice(PlayerNotice::info(
                format!("no se pudo reanudar la posición guardada: {err}"),
            )));
        }
        result
    }
}

#[async_trait]
impl Player for MpvPlayer {
    async fn play(&self, url: &str, start: Option<Duration>) -> Result<()> {
        // Si la combinación cacheada deja de funcionar (YouTube rota sus
        // defensas), se descarta y se recorre el plan completo.
        let mut attempts = self.attempt_plan();
        let mut retried_full_plan = false;
        let mut last_error = String::new();
        let mut blocked = false;
        let mut index = 0;
        while index < attempts.len() {
            let attempt = &attempts[index];
            self.apply_attempt(attempt).await?;
            if index > 0 {
                let _ = self.notify.send(PlayerEvent::Notice(PlayerNotice::info(format!(
                    "stream bloqueado; reintentando: {}…",
                    attempt.label()
                ))));
            }
            match self.try_play(url, start).await {
                Ok(()) => {
                    *lock(&self.cached_attempt) = Some(attempt.clone());
                    return Ok(());
                }
                Err(err) => {
                    if !is_block_error(&err) {
                        return Err(anyhow!(err));
                    }
                    blocked = true;
                    last_error = err;
                    // La caché era un solo intento: si falla, plan completo.
                    if !retried_full_plan && lock(&self.cached_attempt).is_some() {
                        *lock(&self.cached_attempt) = None;
                        attempts = self.attempt_plan();
                        retried_full_plan = true;
                        index = 0;
                        continue;
                    }
                }
            }
            index += 1;
        }
        if blocked {
            bail!(
                "YouTube bloqueó el stream (último error: {last_error}). \
                 Opciones: inicia sesión en YouTube desde algún navegador instalado \
                 (basta una sesión de visitante), instala el proveedor de PO Token \
                 (`pip install bgutil-ytdlp-pot-provider`) o actualiza yt-dlp."
            );
        }
        bail!("{last_error}");
    }

    async fn toggle_pause(&self) -> Result<()> {
        self.command(vec!["cycle".into(), "pause".into()]).await?;
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        self.command(vec!["stop".into()]).await?;
        Ok(())
    }

    async fn seek(&self, seconds: i32) -> Result<()> {
        self.command(vec![
            "seek".into(),
            seconds.to_string().into(),
            "relative".into(),
        ])
        .await?;
        Ok(())
    }

    async fn set_volume(&self, volume: u8) -> Result<()> {
        self.command(vec![
            "set_property".into(),
            "volume".into(),
            f64::from(volume).into(),
        ])
        .await?;
        Ok(())
    }

    async fn status(&self) -> Result<PlaybackStatus> {
        let shared = lock(&self.shared);
        Ok(PlaybackStatus {
            position: shared.position.map(secs_to_duration),
            duration: shared.duration.map(secs_to_duration),
            paused: shared.paused,
            volume: shared.volume.clamp(0, 100) as u8,
        })
    }
}

impl Drop for MpvPlayer {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&self.socket_path);
            if let Some(dir) = self.socket_path.parent() {
                let _ = std::fs::remove_dir(dir);
            }
        }
        #[cfg(not(unix))]
        let _ = &self.socket_path;
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

/// Detecta el plugin bgutil PO Token de yt-dlp: paquete pip instalado o
/// carpeta de plugin en las rutas estándar de yt-dlp.
fn detect_pot_provider() -> bool {
    let pip = std::process::Command::new("pip")
        .args(["show", "bgutil-ytdlp-pot-provider"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if pip {
        return true;
    }
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    ["~/.config/yt-dlp/plugins", "~/.yt-dlp/plugins"].iter().any(|dir| {
        let dir = home.join(dir.trim_start_matches("~/"));
        std::fs::read_dir(&dir)
            .map(|mut entries| {
                entries.any(|e| {
                    e.ok()
                        .is_some_and(|e| e.file_name().to_string_lossy().contains("bgutil"))
                })
            })
            .unwrap_or(false)
    })
}

/// mpv puede reportar time-pos negativo (buffering inicial, seek antes del
/// inicio); from_secs_f64 panicua con negativos, así que se clama a 0.
fn secs_to_duration(seconds: f64) -> Duration {
    Duration::from_secs_f64(seconds.max(0.0))
}

fn ytdl_raw_options(youtube: &Youtube, attempt: &PlayAttempt) -> Option<String> {
    let mut options = Vec::new();
    if let CookieAttempt::Browser(browser) = &attempt.cookies {
        options.push(format!("cookies-from-browser={browser}"));
    }
    if !attempt.client.is_empty() {
        options.push(format!(
            "extractor-args=youtube:player_client={}",
            attempt.client
        ));
    }
    if !youtube.remote_components.is_empty() {
        options.push(format!("remote-components={}", youtube.remote_components));
    }
    (!options.is_empty()).then(|| options.join(","))
}

fn is_block_error(message: &str) -> bool {
    const PATTERNS: &[&str] = &[
        "sign in to confirm",
        "http error 429",
        "http error 403",
        "429 too many requests",
        "403 forbidden",
        "requested format is not available",
        "cookies",
    ];
    let message = message.to_lowercase();
    PATTERNS.iter().any(|pattern| message.contains(pattern))
}

fn clean_stderr_line(line: &str) -> String {
    let line = match line.rsplit_once("ERROR:") {
        Some((_, rest)) => rest.trim(),
        None => line
            .split_once(']')
            .map(|(_, rest)| rest.trim())
            .unwrap_or(line),
    };
    const MAX: usize = 160;
    if line.chars().count() > MAX {
        let cut: String = line.chars().take(MAX).collect();
        format!("{cut}…")
    } else {
        line.to_string()
    }
}

async fn writer_task<W>(mut writer: W, mut rx: mpsc::UnboundedReceiver<Value>)
where
    W: AsyncWrite + Unpin,
{
    while let Some(command) = rx.recv().await {
        let mut line = command.to_string();
        line.push('\n');
        if writer.write_all(line.as_bytes()).await.is_err() {
            break;
        }
        let _ = writer.flush().await;
    }
}

async fn stderr_task<R>(reader: R, shared: Arc<Mutex<Shared>>)
where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        let mut shared = lock(&shared);
        if shared.stderr_tail.len() >= STDERR_TAIL_CAP {
            shared.stderr_tail.pop_front();
        }
        shared.stderr_tail.push_back(line);
    }
}

async fn reader_task<R>(
    reader: R,
    shared: Arc<Mutex<Shared>>,
    notify: mpsc::UnboundedSender<PlayerEvent>,
) where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => handle_message(&line, &shared, &notify),
            Ok(None) => {
                // EOF: la conexión IPC se cerró porque mpv terminó en marcha.
                let _ = notify.send(PlayerEvent::Died);
                break;
            }
            Err(_) => break,
        }
    }
}

fn handle_message(
    line: &str,
    shared: &Arc<Mutex<Shared>>,
    notify: &mpsc::UnboundedSender<PlayerEvent>,
) {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return;
    };
    if let Some(id) = value.get("request_id").and_then(Value::as_u64) {
        let sender = lock(shared).pending.remove(&id);
        if let Some(sender) = sender {
            let _ = sender.send(value);
        }
        return;
    }
    match value.get("event").and_then(Value::as_str) {
        Some("property-change") => {
            let name = value.get("name").and_then(Value::as_str).unwrap_or("");
            let data = value.get("data").cloned().unwrap_or(Value::Null);
            let mut shared = lock(shared);
            match name {
                "time-pos" => shared.position = data.as_f64(),
                "duration" => shared.duration = data.as_f64(),
                "pause" => shared.paused = data.as_bool().unwrap_or(false),
                "volume" => {
                    if let Some(volume) = data.as_f64() {
                        shared.volume = volume.round() as i64;
                    }
                }
                _ => {}
            }
        }
        Some("start-file") => {
            // Marca qué entrada está cargando la espera actual: un end-file
            // rezagado de la entrada anterior (p. ej. de un stop) llega antes
            // que este start-file y no debe resolver la espera.
            let entry = value.get("playlist_entry_id").and_then(Value::as_i64);
            let mut shared = lock(shared);
            if shared.outcome_tx.is_some() {
                shared.pending_entry = entry;
            }
        }
        Some("playback-restart") => {
            let mut shared = lock(shared);
            shared.pending_entry = None;
            let outcome = shared.outcome_tx.take();
            drop(shared);
            if let Some(outcome) = outcome {
                let _ = outcome.send(Ok(()));
            }
        }
        Some("end-file") => {
            let reason = value.get("reason").and_then(Value::as_str).unwrap_or("");
            let entry = value.get("playlist_entry_id").and_then(Value::as_i64);
            let outcome = {
                let mut shared = lock(shared);
                // Con espera armada solo la resuelve el end-file de la
                // entrada que empezó a cargar (start-file fija pending_entry);
                // uno rezagado de la entrada anterior se ignora.
                match (shared.outcome_tx.is_some(), shared.pending_entry, entry) {
                    (true, Some(pending), e) if e != Some(pending) => None,
                    (true, _, _) => {
                        shared.pending_entry = None;
                        shared.outcome_tx.take()
                    }
                    (false, _, _) => None,
                }
            };
            if reason != "error" {
                // stop/eof con una carga en espera: se considera resuelta.
                if let Some(outcome) = outcome {
                    let _ = outcome.send(Ok(()));
                } else if reason == "eof" {
                    let _ = notify.send(PlayerEvent::TrackEnded);
                }
                return;
            }
                let detail = value.get("error").and_then(Value::as_str).unwrap_or("");
                let hint = {
                    let shared = lock(shared);
                    let tail = &shared.stderr_tail;
                    tail.iter()
                        .rev()
                        .find(|line| is_block_error(line))
                        .or_else(|| tail.iter().rev().find(|line| line.contains("ERROR:")))
                        .or_else(|| tail.back())
                        .cloned()
                };
                let message = match (hint, detail) {
                    (Some(hint), _) => clean_stderr_line(&hint),
                    (None, "") => "desconocido".to_string(),
                    (None, detail) => detail.to_string(),
                };
                match outcome {
                    Some(outcome) => {
                        let _ = outcome.send(Err(message));
                    }
                    None => {
                        let _ = notify.send(PlayerEvent::Notice(PlayerNotice::error(
                            format!("error de reproducción: {message}"),
                        )));
                    }
                }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_block_errors() {
        for message in [
            "[youtube] abc: Sign in to confirm you're not a bot",
            "https: HTTP error 429 Too Many Requests",
            "HTTP Error 403: Forbidden",
            "Requested format is not available",
            "Use --cookies-from-browser or --cookies",
        ] {
            assert!(is_block_error(message), "debería ser bloqueo: {message}");
        }
        for message in ["This video is unavailable", "tiempo de espera agotado", "desconocido"] {
            assert!(!is_block_error(message), "no debería ser bloqueo: {message}");
        }
    }

    #[test]
    fn ipc_paths_do_not_collide_between_instances() {
        let first = ipc_server_path();
        let second = ipc_server_path();
        assert_ne!(first, second);
    }

    #[test]
    fn secs_to_duration_clamps_negatives() {
        assert_eq!(secs_to_duration(-2.5), Duration::ZERO);
        assert_eq!(secs_to_duration(0.0), Duration::ZERO);
        assert_eq!(secs_to_duration(1.5), Duration::from_secs_f64(1.5));
    }

    #[test]
    fn attempt_plan_configured_client_first_then_fallbacks() {
        let plan = build_attempt_plan("android", false, &[CookieAttempt::None]);
        let clients: Vec<&str> = plan.iter().map(|a| a.client.as_str()).collect();
        assert_eq!(clients, ["android", "ios", "tv", "web_embedded"]);
    }

    #[test]
    fn attempt_plan_pot_provider_puts_default_client_first() {
        let plan = build_attempt_plan("android", true, &[CookieAttempt::None]);
        assert_eq!(plan[0].client, "");
        assert_eq!(plan[1].client, "android");
    }

    #[test]
    fn attempt_plan_dedupes_configured_client_in_fallbacks() {
        let plan = build_attempt_plan("ios", false, &[CookieAttempt::None]);
        let clients: Vec<&str> = plan.iter().map(|a| a.client.as_str()).collect();
        assert_eq!(clients, ["ios", "android", "tv", "web_embedded"]);
    }

    #[test]
    fn attempt_plan_combines_clients_with_browsers() {
        let cookies = vec![CookieAttempt::None, CookieAttempt::Browser("firefox".into())];
        let plan = build_attempt_plan("android", false, &cookies);
        assert_eq!(plan.len(), 4 * 2, "4 clientes × 2 cookies");
        assert_eq!(plan[0].client, "android");
        assert_eq!(plan[0].cookies, CookieAttempt::None);
        assert_eq!(plan[1].cookies, CookieAttempt::Browser("firefox".into()));
        assert_eq!(plan[2].client, "ios");
    }

    #[test]
    fn attempt_plan_empty_configured_client_not_duplicated_with_pot() {
        // Config "" y PO Token activo: el cliente por defecto sale una vez.
        let plan = build_attempt_plan("", true, &[CookieAttempt::None]);
        let defaults = plan.iter().filter(|a| a.client.is_empty()).count();
        assert_eq!(defaults, 1);
    }

    #[cfg(unix)]
    #[test]
    fn ipc_socket_lives_in_private_dir() {
        let path = ipc_server_path();
        let dir = path.parent().unwrap();
        assert_eq!(dir.parent(), Some(std::env::temp_dir().as_path()));
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .unwrap();
        let mode = std::fs::metadata(dir).unwrap().permissions();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(mode.mode() & 0o777, 0o700);
        let _ = std::fs::remove_dir(dir);
    }
}
