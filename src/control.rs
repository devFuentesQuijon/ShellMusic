//! Socket de control local (RF-63): expone el estado y las acciones de la
//! app en ejecución a clientes del mismo usuario. El cliente de referencia
//! es el puente MCP [`smusic-mcp`](../src/bin/smusic-mcp.rs), que lo traduce
//! a tools para agentes de IA. Protocolo: un JSON por línea en cada
//! dirección (`{"req": …, …}` → `{"ok": true, "data": …}` o
//! `{"ok": false, "error": …}`) sobre un socket Unix en directorio 0700
//! (Linux/macOS) o un named pipe (Windows). Opt-in: `[mcp] enabled = true`.

use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

use crate::providers::Track;

/// Petición entrante por el socket: JSON tal cual y canal por el que la app
/// devuelve la respuesta (el servidor la escribe como línea JSON). Las
/// peticiones cuya respuesta depende de una búsqueda asíncrona guardan el
/// sender en `App::control_pending` y responden al llegar el resultado.
pub struct ControlRequest {
    pub body: Value,
    pub reply: oneshot::Sender<Value>,
}

/// Envoltorio de respuesta afirmativa: `{"ok": true, "data": …}`.
pub fn ok(data: Value) -> Value {
    json!({ "ok": true, "data": data })
}

/// Envoltorio de respuesta de error: `{"ok": false, "error": "…"}`.
pub fn fail(error: impl Into<String>) -> Value {
    json!({ "ok": false, "error": error.into() })
}

/// Representación JSON de una pista para el protocolo (duración en
/// segundos, nombre de proveedor estable).
pub fn track_json(track: &Track) -> Value {
    json!({
        "id": track.id,
        "title": track.title,
        "artist": track.artist,
        "duration_secs": track.duration.map(|d| d.as_secs()),
        "url": track.source_url,
        "provider": track.provider.name(),
    })
}

/// Lista de pistas como array JSON.
pub fn tracks_json(tracks: &[Track]) -> Value {
    Value::Array(tracks.iter().map(track_json).collect())
}

// ── Endpoint ────────────────────────────────────────────────────────────────

/// Dirección del socket de control según el SO.
pub enum Endpoint {
    /// Socket Unix con ruta estable por usuario.
    Unix(std::path::PathBuf),
    /// Named pipe de Windows (`\\.\pipe\…`).
    Pipe(String),
}

/// Endpoint de control de este usuario y SO.
pub fn endpoint() -> Endpoint {
    #[cfg(windows)]
    {
        Endpoint::Pipe(pipe_name())
    }
    #[cfg(not(windows))]
    {
        Endpoint::Unix(socket_path())
    }
}

#[cfg(windows)]
fn pipe_name() -> String {
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".to_string());
    format!(r"\\.\pipe\shellmusic-control-{user}")
}

#[cfg(not(windows))]
fn socket_path() -> std::path::PathBuf {
    socket_path_in(
        std::env::var_os("XDG_RUNTIME_DIR"),
        std::env::var("UID").ok().and_then(|u| u.parse().ok()),
        std::env::temp_dir(),
    )
}

/// Resolución pura (testeable): Linux prefiere `$XDG_RUNTIME_DIR` y cae a
/// `/run/user/{uid}` y luego a un directorio privado en el temporal; macOS
/// usa `$TMPDIR`, que ya es privado del usuario.
#[cfg(not(windows))]
fn socket_path_in(
    xdg_runtime: Option<std::ffi::OsString>,
    uid: Option<u32>,
    temp: std::path::PathBuf,
) -> std::path::PathBuf {
    let base = if cfg!(target_os = "linux") {
        xdg_runtime
            .map(std::path::PathBuf::from)
            .or_else(|| uid.map(|u| std::path::PathBuf::from(format!("/run/user/{u}"))))
            .unwrap_or_else(|| match uid {
                Some(u) => temp.join(format!("shellmusic-{u}")),
                None => temp.join("shellmusic"),
            })
    } else {
        temp
    };
    base.join("shellmusic").join("control.sock")
}

// ── Servidor ────────────────────────────────────────────────────────────────

/// Arranca el servidor de control y acepta conexiones hasta que la app
/// termine. Error si el endpoint no se pudo crear (otra instancia activa,
/// sin permisos): el arranque de la app continúa sin control.
pub async fn spawn_server(tx: mpsc::UnboundedSender<ControlRequest>) -> Result<()> {
    #[cfg(not(windows))]
    {
        let path = socket_path();
        prepare_unix_endpoint(&path)?;
        let listener = tokio::net::UnixListener::bind(&path)
            .with_context(|| format!("no se pudo crear el socket de control {}", path.display()))?;
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        tokio::spawn(serve(stream, tx.clone()));
                    }
                    // Un error de aceptación no debe tumbar la app.
                    Err(_) => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
                }
            }
        });
        Ok(())
    }
    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ServerOptions;
        let name = pipe_name();
        let mut server = ServerOptions::new()
            .create(&name)
            .map_err(|err| anyhow!("no se pudo crear el pipe de control (¿otra instancia activa?): {err}"))?;
        tokio::spawn(async move {
            loop {
                if server.connect().await.is_err() {
                    break;
                }
                let connected = match ServerOptions::new().create(&name) {
                    Ok(next) => std::mem::replace(&mut server, next),
                    Err(_) => break,
                };
                tokio::spawn(serve(connected, tx.clone()));
            }
        });
        Ok(())
    }
}

/// Prepara el directorio (0700) y elimina un socket huérfano de una muerte
/// abrupta. Si el socket responde, hay otra instancia en ejecución: se
/// conserva la suya y se reporta el conflicto.
#[cfg(not(windows))]
fn prepare_unix_endpoint(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if path.exists() {
        if std::os::unix::net::UnixStream::connect(path).is_ok() {
            bail_other_instance()?;
        }
        std::fs::remove_file(path)
            .with_context(|| format!("no se pudo eliminar el socket huérfano {}", path.display()))?;
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("no se pudo crear el directorio {}", dir.display()))?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("no se pudo proteger el directorio {}", dir.display()))?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn bail_other_instance() -> Result<()> {
    Err(anyhow!(
        "ya hay otra instancia de ShellMusic con el control activo; ciérrala o desactiva [mcp]"
    ))
}

/// Sirve una conexión: una petición JSON por línea, una respuesta por
/// petición. El EOF cierra la conexión; la app sigue aceptando otras.
async fn serve<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    tx: mpsc::UnboundedSender<ControlRequest>,
) {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut lines = BufReader::new(reader).lines();
    loop {
        let line = match lines.next_line().await {
            Ok(Some(line)) => line,
            // EOF o error de lectura: el cliente se fue o el socket murió.
            _ => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let body: Value = match serde_json::from_str(trimmed) {
            Ok(body) => body,
            Err(err) => {
                if !write_line(&mut writer, fail(format!("JSON inválido: {err}"))).await {
                    break;
                }
                continue;
            }
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        if tx.send(ControlRequest { body, reply: reply_tx }).is_err() {
            break;
        }
        // Si la app murió, el sender se libera sin valor: respondemos y
        // cerramos en lugar de quedarnos colgados.
        let response = match reply_rx.await {
            Ok(response) => response,
            Err(_) => fail("la app no pudo procesar la petición"),
        };
        if !write_line(&mut writer, response).await {
            break;
        }
    }
}

/// Escribe un JSON como línea (`\n` final). Devuelve false si la conexión se
/// rompió: el caller cierra la conexión.
async fn write_line<W: AsyncWrite + Unpin>(writer: &mut W, value: Value) -> bool {
    let mut bytes = match serde_json::to_vec(&value) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    bytes.push(b'\n');
    writer.write_all(&bytes).await.is_ok() && writer.flush().await.is_ok()
}

// ── Cliente (una petición por conexión) ─────────────────────────────────────

/// Abre una conexión con la app en ejecución, envía `body` y devuelve la
/// respuesta. El timeout corre a cargo del llamador (`tokio::time::timeout`).
pub async fn request(body: Value) -> Result<Value> {
    #[cfg(not(windows))]
    {
        let path = socket_path();
        let stream = tokio::net::UnixStream::connect(&path)
            .await
            .with_context(|| format!("no se pudo conectar al socket de control (¿smusic en ejecución con [mcp] enabled?): {}", path.display()))?;
        roundtrip(stream, body).await
    }
    #[cfg(windows)]
    {
        let stream = tokio::net::windows::named_pipe::ClientOptions::new()
            .open(pipe_name())
            .map_err(|err| anyhow!("no se pudo conectar al pipe de control (¿smusic en ejecución con [mcp] enabled?): {err}"))?;
        roundtrip(stream, body).await
    }
}

/// Envía `body` como línea JSON y espera la línea de respuesta.
async fn roundtrip<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S, body: Value) -> Result<Value> {
    let mut bytes = serde_json::to_vec(&body)?;
    bytes.push(b'\n');
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    let mut line = String::new();
    let read = BufReader::new(stream).read_line(&mut line).await?;
    if read == 0 {
        return Err(anyhow!("la app cerró la conexión sin responder"));
    }
    serde_json::from_str(line.trim()).context("respuesta de control inválida")
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn sample_track() -> Track {
        Track {
            id: "abc".to_string(),
            title: "One More Time".to_string(),
            artist: "Daft Punk".to_string(),
            duration: Some(Duration::from_secs(320)),
            source_url: "https://youtu.be/abc".to_string(),
            provider: crate::providers::ProviderKind::Youtube,
        }
    }

    #[test]
    fn envelopes_round_trip() {
        let ok = ok(json!({"x": 1}));
        assert_eq!(ok["ok"], true);
        assert_eq!(ok["data"]["x"], 1);
        let err = fail("razón");
        assert_eq!(err["ok"], false);
        assert_eq!(err["error"], "razón");
    }

    #[test]
    fn track_json_uses_stable_provider_name() {
        let json = track_json(&sample_track());
        assert_eq!(json["provider"], "youtube");
        assert_eq!(json["duration_secs"], 320);
        assert_eq!(json["title"], "One More Time");
        assert_eq!(json["url"], "https://youtu.be/abc");
        let list = tracks_json(&[sample_track()]);
        assert_eq!(list.as_array().unwrap().len(), 1);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn socket_path_prefers_xdg_then_uid_then_temp() {
        let xdg = Some(std::ffi::OsString::from("/run/user/1000"));
        assert_eq!(
            socket_path_in(xdg, Some(1000), std::path::PathBuf::from("/tmp")),
            std::path::PathBuf::from("/run/user/1000/shellmusic/control.sock")
        );
        // Sin XDG pero con uid: /run/user/{uid}.
        assert_eq!(
            socket_path_in(None, Some(1000), std::path::PathBuf::from("/tmp")),
            std::path::PathBuf::from("/run/user/1000/shellmusic/control.sock")
        );
        // Sin nada: directorio privado en el temporal.
        assert_eq!(
            socket_path_in(None, None, std::path::PathBuf::from("/tmp")),
            std::path::PathBuf::from("/tmp/shellmusic/shellmusic/control.sock")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn serve_replies_each_request_line() {
        let dir = std::env::temp_dir().join(format!(
            "shellmusic-control-test-{}-{:016x}",
            std::process::id(),
            crate::state_file::random_suffix()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("control.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel::<ControlRequest>();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(serve(stream, tx.clone()));
            }
        });
        let responder = tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                let n = req.body["n"].as_u64().unwrap_or(0);
                let _ = req.reply.send(ok(json!({ "double": n * 2 })));
            }
        });
        let stream = tokio::net::UnixStream::connect(&path).await.unwrap();
        let response = roundtrip(stream, json!({ "req": "test", "n": 21 })).await.unwrap();
        assert_eq!(response["ok"], true);
        assert_eq!(response["data"]["double"], 42);
        // JSON inválido: error descriptivo, conexión viva.
        let mut stream = tokio::net::UnixStream::connect(&path).await.unwrap();
        use tokio::io::AsyncWriteExt;
        stream.write_all(b"no json\n").await.unwrap();
        stream.flush().await.unwrap();
        let mut line = String::new();
        let mut reader = BufReader::new(stream);
        reader.read_line(&mut line).await.unwrap();
        let parsed: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(parsed["ok"], false);
        assert!(parsed["error"].as_str().unwrap().contains("JSON inválido"));
        responder.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
