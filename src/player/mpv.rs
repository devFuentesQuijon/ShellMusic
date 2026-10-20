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
            random_suffix()
        ))
        .join("mpv.sock")
}

/// Aleatorio por proceso sin dependencias: las claves de RandomState se
/// generan con semilla del SO en cada llamada.
fn random_suffix() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
}

#[cfg(windows)]
fn ipc_server_path() -> PathBuf {
    let instance = INSTANCE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    PathBuf::from(format!(
        r"\\.\pipe\shellmusic-mpv-{}-{instance}-{:016x}",
        std::process::id(),
        random_suffix()
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

impl CookieAttempt {
    fn label(&self) -> &str {
        match self {
            Self::None => "sin cookies",
            Self::Browser(browser) => browser,
        }
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
}

pub struct MpvPlayer {
    child: Child,
    outgoing: mpsc::UnboundedSender<Value>,
    notify: mpsc::UnboundedSender<PlayerEvent>,
    shared: Arc<Mutex<Shared>>,
    socket_path: PathBuf,
    youtube: Youtube,
    cookies_mode: CookiesMode,
    cached_attempt: Mutex<Option<CookieAttempt>>,
    detected_browsers: Mutex<Option<Vec<String>>>,
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

    fn attempt_plan(&self) -> Vec<CookieAttempt> {
        match self.cookies_mode {
            CookiesMode::Disabled => vec![CookieAttempt::None],
            CookiesMode::Fixed => vec![CookieAttempt::Browser(
                self.youtube.cookies_from_browser.clone(),
            )],
            CookiesMode::Auto => {
                if let Some(cached) = lock(&self.cached_attempt).clone() {
                    return vec![cached];
                }
                let mut attempts = vec![CookieAttempt::None];
                attempts.extend(
                    self.installed_browsers().into_iter().map(CookieAttempt::Browser),
                );
                attempts
            }
        }
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

    async fn apply_attempt(&self, attempt: &CookieAttempt) -> Result<()> {
        let options = ytdl_raw_options(&self.youtube, attempt).unwrap_or_default();
        self.command(vec![
            "set_property".into(),
            "ytdl-raw-options".into(),
            options.into(),
        ])
        .await?;
        Ok(())
    }

    async fn try_play(&self, url: &str) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        {
            let mut shared = lock(&self.shared);
            shared.stderr_tail.clear();
            shared.outcome_tx = Some(tx);
        }
        self.command(vec!["loadfile".into(), url.into()])
            .await
            .map_err(|err| err.to_string())?;
        match tokio::time::timeout(PLAY_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            // El emisor se descartó: otro play() tomó el relevo.
            Ok(Err(_)) => Ok(()),
            Err(_) => {
                lock(&self.shared).outcome_tx = None;
                Err("tiempo de espera agotado al cargar el stream".to_string())
            }
        }
    }
}

#[async_trait]
impl Player for MpvPlayer {
    async fn play(&self, url: &str) -> Result<()> {
        let attempts = self.attempt_plan();
        let mut last_error = String::new();
        let mut blocked = false;
        for (index, attempt) in attempts.iter().enumerate() {
            self.apply_attempt(attempt).await?;
            if index > 0 {
                let _ = self.notify.send(PlayerEvent::Notice(PlayerNotice::info(format!(
                    "stream bloqueado; reintentando con cookies de {}…",
                    attempt.label()
                ))));
            }
            match self.try_play(url).await {
                Ok(()) => {
                    if self.cookies_mode == CookiesMode::Auto {
                        *lock(&self.cached_attempt) = Some(attempt.clone());
                    }
                    return Ok(());
                }
                Err(err) => {
                    if !is_block_error(&err) {
                        return Err(anyhow!(err));
                    }
                    blocked = true;
                    last_error = err;
                }
            }
        }
        if blocked {
            bail!(
                "YouTube bloqueó el stream (último error: {last_error}). \
                 Inicia sesión en YouTube desde algún navegador instalado \
                 (basta una sesión de visitante, no hace falta cuenta) y reintenta."
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
            position: shared.position.map(Duration::from_secs_f64),
            duration: shared.duration.map(Duration::from_secs_f64),
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

fn ytdl_raw_options(youtube: &Youtube, attempt: &CookieAttempt) -> Option<String> {
    let mut options = Vec::new();
    if let CookieAttempt::Browser(browser) = attempt {
        options.push(format!("cookies-from-browser={browser}"));
    }
    if !youtube.player_client.is_empty() {
        options.push(format!(
            "extractor-args=youtube:player_client={}",
            youtube.player_client
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
                let _ = notify.send(PlayerEvent::Notice(PlayerNotice::error("mpv se cerró inesperadamente")));
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
        Some("playback-restart") => {
            let outcome = lock(shared).outcome_tx.take();
            if let Some(outcome) = outcome {
                let _ = outcome.send(Ok(()));
            }
        }
        Some("end-file") => {
            let reason = value.get("reason").and_then(Value::as_str).unwrap_or("");
            if reason != "error" {
                // stop/eof con una carga en espera: se considera resuelta.
                let outcome = lock(shared).outcome_tx.take();
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
                let outcome = lock(shared).outcome_tx.take();
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
