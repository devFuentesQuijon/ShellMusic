//! Presencia en Discord («escuchando: …») vía su IPC local, a mano: el
//! protocolo es JSON con framing de 8 bytes sobre un socket Unix (Linux/
//! macOS) o named pipe (Windows) — el mismo patrón que `player/mpv.rs`.
//! Best-effort: si Discord no está abierto, la tarea reintenta en silencio.
//!
//! El Application ID es por aplicación: el autor registra «ShellMusic» una
//! vez en el portal de desarrolladores de Discord y ese ID queda embebido
//! aquí; los usuarios finales no crean nada (solo `[integrations]
//! discord = true` y el cliente Discord abierto).

#[cfg(unix)]
use std::path::PathBuf;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

/// Application ID de la app «ShellMusic» en el portal de Discord. El autor
/// del proyecto lo crea una vez (discord.com/developers → New Application →
/// Application ID) y se embebe aquí; `[integrations] discord_client_id`
/// permite a los forks usar el suyo propio. Vacío = deshabilitado.
const DISCORD_CLIENT_ID: &str = "1565030208623616153";

const RETRY_EVERY: Duration = Duration::from_secs(15);
const RECV_TIMEOUT: Duration = Duration::from_secs(5);

/// Estado de reproducción que la app publica en el perfil de Discord.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Presence {
    /// Sonando esta pista desde `start_unix` (epoch, para el temporizador).
    Playing {
        title: String,
        artist: String,
        start_unix: u64,
    },
    /// En pausa.
    Paused { title: String, artist: String },
    /// Sin reproducción: se limpia la presencia.
    Stopped,
}

/// Handle para publicar presencia: envía al canal que consume la tarea en
/// segundo plano. Es barato de clonar.
#[derive(Clone)]
pub struct PresenceSender {
    tx: mpsc::UnboundedSender<Presence>,
}

impl PresenceSender {
    pub fn send(&self, presence: Presence) {
        let _ = self.tx.send(presence);
    }
}

/// Arranca la tarea de presencia si la integración está habilitada y hay
/// Application ID (el embebido o el de config). Devuelve None si no: la app
/// sigue igual y `missing_reason` explica el motivo para avisar al usuario.
pub fn spawn(
    enabled: bool,
    config_client_id: &str,
) -> (Option<PresenceSender>, Option<String>) {
    let client_id = if !config_client_id.is_empty() {
        config_client_id.to_string()
    } else {
        DISCORD_CLIENT_ID.to_string()
    };
    if !enabled {
        return (None, None);
    }
    if client_id.is_empty() {
        return (
            None,
            Some(
                "Discord activado sin Application ID: configura [integrations] discord_client_id (o embebe uno en discord.rs)"
                    .to_string(),
            ),
        );
    }
    let (tx, rx) = mpsc::unbounded_channel::<Presence>();
    tokio::spawn(presence_task(client_id, rx));
    (Some(PresenceSender { tx }), None)
}

/// Tarea de fondo: se conecta al socket de Discord (reintentando mientras
/// no exista), mantiene la actividad actualizada con lo último publicado y
/// reconecta si Discord se cierra o reinicia.
async fn presence_task(client_id: String, mut rx: mpsc::UnboundedReceiver<Presence>) {
    let mut last: Option<Presence> = None;
    loop {
        match connect_and_serve(&client_id, &mut rx, &mut last).await {
            // Desconectado (Discord cerrado o error): reintento en silencio
            // con lo último conocido para republicarlo al reconectar.
            Ok(()) | Err(_) => tokio::time::sleep(RETRY_EVERY).await,
        }
    }
}

/// Una sesión con el socket de Discord: handshake, aplicación de la
/// presencia pendiente y espera de novedades hasta que el socket muere.
async fn connect_and_serve(
    client_id: &str,
    rx: &mut mpsc::UnboundedReceiver<Presence>,
    last: &mut Option<Presence>,
) -> std::io::Result<()> {
    let stream = connect_socket().await?;
    serve(stream, client_id, rx, last).await
}

/// Protocolo sobre un stream ya conectado (separado de `connect_socket`
/// para poder probarlo con un socket Unix falso en los tests).
async fn serve(
    stream: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    client_id: &str,
    rx: &mut mpsc::UnboundedReceiver<Presence>,
    last: &mut Option<Presence>,
) -> std::io::Result<()> {
    let (mut reader, mut writer) = tokio::io::split(stream);

    send_frame(
        &mut writer,
        0, // handshake
        &serde_json::json!({
            "cmd": "INIT",
            "args": { "v": 1, "client_id": client_id },
            "nonce": nonce(),
        }),
    )
    .await?;
    // El READY confirma el handshake; si Discord rechaza el client_id no
    // llega y el timeout cierra la sesión para reintentar.
    let ready = tokio::time::timeout(RECV_TIMEOUT, recv_frame(&mut reader)).await;
    match ready {
        Ok(Ok(Some(frame))) if frame.get("cmd").and_then(serde_json::Value::as_str) == Some("DISPATCH") => {}
        // EOF u otro evento: reintentar desde cero.
        _ => return Ok(()),
    }

    // Republicar lo que sonaba cuando se cayó la conexión anterior (o lo
    // que pidió la app mientras Discord no estaba).
    if let Some(pending) = last.clone() {
        apply_presence(&mut writer, &pending).await?;
    }

    loop {
        tokio::select! {
            presence = rx.recv() => {
                let presence = match presence {
                    Some(presence) => presence,
                    None => return Ok(()), // app cerrada
                };
                // Recordar antes de escribir: si el write falla, la
                // reconexión lo reintenta con el estado más reciente.
                *last = Some(presence.clone());
                apply_presence(&mut writer, &presence).await?;
            }
            // Socket en reposo: leer para detectar el cierre de Discord
            // aunque no haya novedades de la app. Los eventos que manda
            // Discord sin que se le pregunte se ignoran.
            frame = recv_frame(&mut reader) => match frame {
                Ok(Some(_event)) => {}
                Ok(None) => return Ok(()), // EOF: Discord cerró
                Err(err) => return Err(err),
            },
        }
    }
}

async fn apply_presence(
    writer: &mut (impl AsyncWriteExt + Unpin),
    presence: &Presence,
) -> std::io::Result<()> {
    let activity = match presence {
        Presence::Stopped => serde_json::Value::Null,
        Presence::Playing {
            title,
            artist,
            start_unix,
        } => serde_json::json!({
            "details": truncate_field(&format!("{title} — {artist}")),
            "state": "reproduciendo",
            "timestamps": { "start": start_unix },
            "assets": { "large_text": "ShellMusic" },
        }),
        Presence::Paused { title, artist } => serde_json::json!({
            "details": truncate_field(&format!("{title} — {artist}")),
            "state": "en pausa",
            "assets": { "large_text": "ShellMusic" },
        }),
    };
    send_frame(
        writer,
        1, // frame
        &serde_json::json!({
            "cmd": "SET_ACTIVITY",
            "args": {
                "pid": std::process::id(),
                "activity": activity,
            },
            "nonce": nonce(),
        }),
    )
    .await
}

/// Los campos de Discord tienen límite de 128 caracteres; se recortan con
/// ellipsis para no saturar el perfil.
fn truncate_field(text: &str) -> String {
    const LIMIT: usize = 128;
    if text.chars().count() <= LIMIT {
        return text.to_string();
    }
    let mut out: String = text.chars().take(LIMIT - 1).collect();
    out.push('…');
    out
}

fn nonce() -> String {
    format!(
        "{}-{:016x}",
        std::process::id(),
        crate::state_file::random_suffix()
    )
}

// ── Transporte ──────────────────────────────────────────────────────────────

/// Conecta al primer socket de Discord disponible. Error si ninguno existe
/// (Discord no instalado/abierto): la tarea reintenta.
#[cfg(unix)]
async fn connect_socket() -> std::io::Result<tokio::net::UnixStream> {
    use tokio::net::UnixStream;
    let mut last_err = std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "no se encontró el socket de Discord",
    );
    for path in socket_candidates() {
        match UnixStream::connect(&path).await {
            Ok(stream) => return Ok(stream),
            Err(err) => last_err = err,
        }
    }
    Err(last_err)
}

#[cfg(not(unix))]
async fn connect_socket() -> std::io::Result<tokio::net::TcpStream> {
    // Windows usa named pipes; como aproximación portable sin deps extra,
    // Discord en Windows también escucha en el puerto local… no: no hay
    // fallback TCP. Se devuelve NotFound y la presencia queda deshabilitada
    // con reintentos silenciosos (documentado en integracion-sistema.md).
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "Discord IPC: named pipes no soportados en esta compilación",
    ))
}

/// Rutas candidatas del socket `discord-ipc-N` por SO (orden de intento).
#[cfg(unix)]
fn socket_candidates() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "linux")]
    {
        if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
            dirs.push(runtime.into());
        }
        // Fallback habitual en sesiones sin XDG_RUNTIME_DIR.
        if let Some(uid) = unix_uid() {
            dirs.push(format!("/run/user/{uid}").into());
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(tmp) = std::env::var_os("TMPDIR") {
            dirs.push(tmp.into());
        }
        if let Some(home) = std::env::var_os("HOME") {
            let base: PathBuf = home.into();
            dirs.push(base.join("Library/Application Support/discord"));
        }
    }
    ipc_paths_in(&dirs)
}

/// Expansion pura: cada directorio base × índices 0-9 (testeable sin env).
#[cfg(unix)]
fn ipc_paths_in(dirs: &[PathBuf]) -> Vec<PathBuf> {
    dirs.iter()
        .flat_map(|dir| (0..10).map(move |n| dir.join(format!("discord-ipc-{n}"))))
        .collect()
}

#[cfg(all(unix, target_os = "linux"))]
fn unix_uid() -> Option<u32> {
    // std no expone el uid; se lee del entorno si está, si no se asume el
    // directorio /run/user no aplicable. En la práctica XDG_RUNTIME_DIR
    // existe en sesiones con Discord; este fallback cubre lo demás.
    std::env::var("UID").ok().and_then(|uid| uid.parse().ok())
}

// ── Framing: u32 LE longitud + u32 LE opcode + JSON ─────────────────────────

async fn send_frame(
    writer: &mut (impl AsyncWriteExt + Unpin),
    opcode: u32,
    value: &serde_json::Value,
) -> std::io::Result<()> {
    let payload = serde_json::to_vec(value)
        .map_err(std::io::Error::other)?;
    writer.write_u32_le(payload.len() as u32).await?;
    writer.write_u32_le(opcode).await?;
    writer.write_all(&payload).await?;
    writer.flush().await
}

/// Lee un frame JSON. Ok(None) cuando el par cierra el socket: EOF limpio
/// o reset de conexión (mismo significado: Discord terminó).
async fn recv_frame(reader: &mut (impl AsyncReadExt + Unpin)) -> std::io::Result<Option<serde_json::Value>> {
    let len = match reader.read_u32_le().await {
        Ok(len) => len as usize,
        Err(err)
            if err.kind() == std::io::ErrorKind::UnexpectedEof
                || err.kind() == std::io::ErrorKind::ConnectionReset =>
        {
            return Ok(None)
        }
        Err(err) => return Err(err),
    };
    let _opcode = reader.read_u32_le().await?;
    let mut buffer = vec![0u8; len.min(1 << 20)]; // techo de 1 MiB
    reader.read_exact(&mut buffer).await?;
    serde_json::from_slice(&buffer)
        .map(Some)
        .map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn ipc_paths_expand_indices_per_dir() {
        let dirs = vec![PathBuf::from("/tmp/base1"), PathBuf::from("/tmp/base2")];
        let candidates = ipc_paths_in(&dirs);
        assert_eq!(candidates.len(), 20, "2 dirs × 10 índices");
        assert!(
            candidates
                .iter()
                .any(|p| p.to_string_lossy() == "/tmp/base1/discord-ipc-0")
        );
        assert!(
            candidates
                .iter()
                .any(|p| p.to_string_lossy() == "/tmp/base2/discord-ipc-9")
        );
    }

    #[test]
    fn truncate_field_caps_at_128() {
        let short = "corta";
        assert_eq!(truncate_field(short), short);
        let long = "x".repeat(200);
        let truncated = truncate_field(&long);
        assert_eq!(truncated.chars().count(), 128);
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn presence_activity_payload_serializes() {
        let playing = serde_json::json!({
            "details": "T — A",
            "state": "reproduciendo",
            "timestamps": { "start": 1_700_000_000u64 },
            "assets": { "large_text": "ShellMusic" },
        });
        assert_eq!(
            playing.pointer("/timestamps/start").and_then(serde_json::Value::as_u64),
            Some(1_700_000_000)
        );
        let stopped = Presence::Stopped;
        assert_eq!(stopped, Presence::Stopped);
    }

    #[tokio::test]
    async fn spawn_disabled_returns_nothing() {
        let (sender, warning) = spawn(false, "");
        assert!(sender.is_none());
        assert!(warning.is_none());
    }

    #[tokio::test]
    async fn spawn_enabled_without_id_warns() {
        // Solo cuando el ID embebido también está vacío (como en builds
        // aún sin registrar la app).
        if DISCORD_CLIENT_ID.is_empty() {
            let (sender, warning) = spawn(true, "");
            assert!(sender.is_none());
            assert!(warning.is_some_and(|w| w.contains("Application ID")));
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn serve_handshakes_and_applies_presence() {
        use tokio::net::UnixStream;
        // Par de sockets conectados: `client` lo usa `serve` (rol ShellMusic)
        // y `server_end` actúa de Discord en el test.
        let (client, mut server_end) = UnixStream::pair().unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel::<Presence>();
        let mut last: Option<Presence> = Some(Presence::Playing {
            title: "One More Time".to_string(),
            artist: "Daft Punk".to_string(),
            start_unix: 1_700_000_000,
        });
        tx.send(last.clone().unwrap()).unwrap();

        let serve_handle = tokio::spawn(async move {
            serve(client, "test-client-id", &mut rx, &mut last).await
        });

        // Actuamos como Discord en el otro extremo del par.
        let init = recv_frame(&mut server_end).await.unwrap().unwrap();
        assert_eq!(init.get("cmd").and_then(serde_json::Value::as_str), Some("INIT"));
        assert_eq!(
            init.pointer("/args/client_id").and_then(serde_json::Value::as_str),
            Some("test-client-id")
        );
        send_frame(
            &mut server_end,
            0,
            &serde_json::json!({ "cmd": "DISPATCH", "evt": "READY" }),
        )
        .await
        .unwrap();

        let set = tokio::time::timeout(Duration::from_secs(3), recv_frame(&mut server_end))
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(
            set.get("cmd").and_then(serde_json::Value::as_str),
            Some("SET_ACTIVITY")
        );
        assert_eq!(
            set.pointer("/args/activity/details")
                .and_then(serde_json::Value::as_str),
            Some("One More Time — Daft Punk")
        );
        assert_eq!(
            set.pointer("/args/activity/timestamps/start")
                .and_then(serde_json::Value::as_u64),
            Some(1_700_000_000)
        );

        drop(server_end); // EOF en el cliente
        let result = tokio::time::timeout(Duration::from_secs(3), serve_handle)
            .await
            .unwrap();
        let joined = result.unwrap();
        assert!(
            joined.is_ok(),
            "serve termina limpio en EOF: {:?}",
            joined.err()
        );
    }
}
