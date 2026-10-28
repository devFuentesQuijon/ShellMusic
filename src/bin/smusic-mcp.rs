//! Puente MCP (Model Context Protocol) para ShellMusic (RF-64): servidor MCP
//! mínimo sobre stdio —un mensaje JSON-RPC 2.0 por línea— que traduce cada
//! tool en una petición del socket de control de la app en ejecución
//! (`crate::control`, RF-63). Sin SDK: solo `serde_json`, ya dependencia del
//! paquete. Configúralo en tu agente como comando `smusic-mcp`.

use std::future::Future;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Techo de espera de una tool contra la app (las búsquedas tardan lo que
/// tardan yt-dlp y la red).
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Una tool MCP: nombre, descripción para el agente, esquema JSON de sus
/// argumentos y traducción a petición del socket de control.
struct Tool {
    name: &'static str,
    description: &'static str,
    schema: &'static str,
    /// Cuerpo de la petición de control a partir de los argumentos.
    build: fn(&Value) -> Result<Value, String>,
}

const TOOLS: &[Tool] = &[
    Tool {
        name: "search",
        description: "Busca música (YouTube Music, SoundCloud, Bandcamp por URL) y devuelve la lista de resultados.",
        schema: r#"{"type":"object","properties":{"query":{"type":"string","description":"Texto de búsqueda o URL"}},"required":["query"]}"#,
        build: |args| str_arg(args, "query").map(|query| json!({ "req": "search", "query": query })),
    },
    Tool {
        name: "play",
        description: "Busca y reproduce la primera coincidencia inmediatamente.",
        schema: r#"{"type":"object","properties":{"query":{"type":"string","description":"Texto de búsqueda o URL"}},"required":["query"]}"#,
        build: |args| str_arg(args, "query").map(|query| json!({ "req": "play", "query": query })),
    },
    Tool {
        name: "now_playing",
        description: "Estado actual: pista en reproducción, posición, volumen, cola, radio y repetición.",
        schema: r#"{"type":"object","properties":{}}"#,
        build: |_| Ok(json!({ "req": "now_playing" })),
    },
    Tool {
        name: "queue_list",
        description: "Devuelve la cola de reproducción con el índice de la pista actual.",
        schema: r#"{"type":"object","properties":{}}"#,
        build: |_| Ok(json!({ "req": "queue_list" })),
    },
    Tool {
        name: "queue_add",
        description: "Busca y encola la primera coincidencia (sin reproducirla aún).",
        schema: r#"{"type":"object","properties":{"query":{"type":"string","description":"Texto de búsqueda o URL"}},"required":["query"]}"#,
        build: |args| str_arg(args, "query").map(|query| json!({ "req": "queue_add", "query": query })),
    },
    Tool {
        name: "queue_remove",
        description: "Elimina la pista de la cola con el índice dado (0-based).",
        schema: r#"{"type":"object","properties":{"index":{"type":"integer","minimum":0}},"required":["index"]}"#,
        build: |args| u64_arg(args, "index").map(|index| json!({ "req": "queue_remove", "index": index })),
    },
    Tool {
        name: "queue_play",
        description: "Reproduce la pista de la cola con el índice dado (0-based), sin buscar.",
        schema: r#"{"type":"object","properties":{"index":{"type":"integer","minimum":0}},"required":["index"]}"#,
        build: |args| u64_arg(args, "index").map(|index| json!({ "req": "queue_play", "index": index })),
    },
    Tool {
        name: "queue_clear",
        description: "Vacía la cola y detiene la reproducción.",
        schema: r#"{"type":"object","properties":{}}"#,
        build: |_| Ok(json!({ "req": "queue_clear" })),
    },
    Tool {
        name: "control",
        description: "Ejecuta un comando de la paleta de ShellMusic: pause, stop, next, prev, \"vol up\", \"vol down\", \"vol <0-100>\", \"theme <nombre>\", \"radio on\", \"radio off\", stats, playlists, help, quit.",
        schema: r#"{"type":"object","properties":{"command":{"type":"string","description":"Comando de la paleta (:)"}},"required":["command"]}"#,
        build: |args| str_arg(args, "command").map(|command| json!({ "req": "control", "command": command })),
    },
    Tool {
        name: "radio",
        description: "Activa o desactiva el modo radio (encola pistas relacionadas al agotarse la cola).",
        schema: r#"{"type":"object","properties":{"enabled":{"type":"boolean"}},"required":["enabled"]}"#,
        build: |args| bool_arg(args, "enabled").map(|enabled| json!({ "req": "radio", "enabled": enabled })),
    },
    Tool {
        name: "playlists_list",
        description: "Lista las playlists locales con su número de pistas.",
        schema: r#"{"type":"object","properties":{}}"#,
        build: |_| Ok(json!({ "req": "playlists_list" })),
    },
    Tool {
        name: "playlist_play",
        description: "Sustituye la cola por la playlist indicada y reproduce su primera pista.",
        schema: r#"{"type":"object","properties":{"name":{"type":"string","description":"Nombre de la playlist"}},"required":["name"]}"#,
        build: |args| str_arg(args, "name").map(|name| json!({ "req": "playlist_play", "name": name })),
    },
    Tool {
        name: "favorites_list",
        description: "Devuelve la lista de favoritos.",
        schema: r#"{"type":"object","properties":{}}"#,
        build: |_| Ok(json!({ "req": "favorites_list" })),
    },
];

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("falta el argumento «{key}» (string no vacío)"))
}

fn u64_arg(args: &Value, key: &str) -> Result<u64, String> {
    args.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("falta el argumento «{key}» (entero ≥ 0)"))
}

fn bool_arg(args: &Value, key: &str) -> Result<bool, String> {
    args.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("falta el argumento «{key}» (booleano)"))
}

fn find_tool(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|tool| tool.name == name)
}

fn tools_list() -> Value {
    Value::Array(
        TOOLS
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": serde_json::from_str::<Value>(tool.schema)
                        .expect("esquemas de tools embebidos son JSON válido"),
                })
            })
            .collect(),
    )
}

// ── Mensajes JSON-RPC 2.0 ───────────────────────────────────────────────────

fn result_response(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: &Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message.into() },
    })
}

/// Procesa una línea de entrada (un mensaje JSON-RPC). `control` ejecuta la
/// petición contra la app y se inyecta para los tests. Devuelve la línea de
/// respuesta, o None si el mensaje no requiere respuesta (notificaciones).
async fn process_line<F, Fut>(line: &str, control: &mut F) -> Option<String>
where
    F: FnMut(Value) -> Fut,
    Fut: Future<Output = Result<Value, String>>,
{
    let message: Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(err) => {
            let response = error_response(&Value::Null, -32700, format!("JSON inválido: {err}"));
            return Some(response.to_string());
        }
    };
    // Notificaciones: sin id no se responde (MCP las usa para el handshake).
    let id = message.get("id").cloned()?;
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return Some(error_response(&id, -32600, "falta «method»").to_string());
    };
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let response = match method {
        "initialize" => result_response(&id, {
            let version = params
                .get("protocolVersion")
                .cloned()
                .unwrap_or_else(|| json!("2024-11-05"));
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "shellmusic-mcp", "version": env!("CARGO_PKG_VERSION") },
            })
        }),
        "ping" => result_response(&id, json!({})),
        "tools/list" => result_response(&id, json!({ "tools": tools_list() })),
        "tools/call" => match call_tool(&params, control).await {
            Ok(result) => result_response(&id, result),
            // Parámetros de la tool inválidos: error JSON-RPC (-32602), no
            // resultado de tool.
            Err((code, message)) => error_response(&id, code, message),
        },
        other => error_response(&id, -32601, format!("método desconocido: {other}")),
    };
    Some(response.to_string())
}

/// Ejecuta una `tools/call`: valida nombre y argumentos (error JSON-RPC
/// -32602 si fallan), habla con la app y empaqueta el resultado MCP (texto;
/// `isError` en fallos de la app o del transporte).
async fn call_tool<F, Fut>(params: &Value, control: &mut F) -> Result<Value, (i64, String)>
where
    F: FnMut(Value) -> Fut,
    Fut: Future<Output = Result<Value, String>>,
{
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return Err((-32602, "falta «name» de la tool".to_string()));
    };
    let Some(tool) = find_tool(name) else {
        return Err((-32602, format!("tool desconocida: {name}")));
    };
    let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let body = (tool.build)(&arguments).map_err(|err| (-32602, err))?;
    let outcome = tokio::time::timeout(CALL_TIMEOUT, control(body)).await;
    Ok(match outcome {
        Ok(Ok(response)) if response.get("ok") == Some(&Value::Bool(true)) => {
            let data = response.get("data").cloned().unwrap_or(Value::Null);
            json!({ "content": [{ "type": "text", "text": data.to_string() }] })
        }
        Ok(Ok(response)) => {
            let error = response
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("error desconocido de ShellMusic")
                .to_string();
            json!({ "content": [{ "type": "text", "text": error }], "isError": true })
        }
        Ok(Err(err)) => json!({ "content": [{ "type": "text", "text": err }], "isError": true }),
        Err(_) => json!({
            "content": [{ "type": "text", "text": "tiempo agotado esperando a ShellMusic" }],
            "isError": true,
        }),
    })
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut control = |body: Value| async move {
        shellmusic::control::request(body)
            .await
            .map_err(|err| format!("{err:#}"))
    };
    let mut stdout = tokio::io::stdout();
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let Some(response) = process_line(&line, &mut control).await else {
            continue;
        };
        let mut bytes = response.into_bytes();
        bytes.push(b'\n');
        if stdout.write_all(&bytes).await.is_err() || stdout.flush().await.is_err() {
            break;
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Control falso: respuestas enlatadas según la petición.
    fn fake_control() -> impl FnMut(Value) -> std::future::Ready<Result<Value, String>> {
        |body: Value| {
            let response = match body["req"].as_str() {
                Some("now_playing") => Ok(json!({
                    "ok": true,
                    "data": { "track": null, "paused": true, "volume": 70 }
                })),
                Some("search") => Ok(json!({ "ok": true, "data": { "results": [1, 2, 3] } })),
                Some("queue_remove") => Ok(json!({ "ok": false, "error": "índice 9 fuera de la cola (3 pistas)" })),
                _ => Err("app no disponible".to_string()),
            };
            std::future::ready(response)
        }
    }

    fn parse(response: String) -> Value {
        serde_json::from_str(&response).expect("respuesta JSON-RPC")
    }

    #[tokio::test]
    async fn initialize_mirrors_protocol_version() {
        let mut control = fake_control();
        let line = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#;
        let response = parse(process_line(line, &mut control).await.unwrap());
        assert_eq!(response["id"], 1);
        assert_eq!(response["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(response["result"]["serverInfo"]["name"], "shellmusic-mcp");
    }

    #[tokio::test]
    async fn tools_list_declares_all_tools_with_schemas() {
        let mut control = fake_control();
        let line = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
        let response = parse(process_line(line, &mut control).await.unwrap());
        let tools = response["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), TOOLS.len());
        for tool in tools {
            assert!(tool["name"].is_string());
            assert!(tool["inputSchema"]["type"] == "object");
        }
        assert!(tools.iter().any(|t| t["name"] == "search"));
        assert!(tools.iter().any(|t| t["name"] == "playlist_play"));
    }

    #[tokio::test]
    async fn tools_call_maps_control_response_to_text_content() {
        let mut control = fake_control();
        let line = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search","arguments":{"query":"daft punk"}}}"#;
        let response = parse(process_line(line, &mut control).await.unwrap());
        assert_eq!(response["result"]["isError"], Value::Null);
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"results\""), "texto: {text}");
    }

    #[tokio::test]
    async fn tools_call_marks_control_error_as_is_error() {
        let mut control = fake_control();
        let line = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"queue_remove","arguments":{"index":9}}}"#;
        let response = parse(process_line(line, &mut control).await.unwrap());
        assert_eq!(response["result"]["isError"], true);
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("fuera de la cola"));
    }

    #[tokio::test]
    async fn tools_call_transport_failure_is_error_not_crash() {
        let mut control = fake_control();
        let line = r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"now_playing","arguments":{}}}"#;
        let response = parse(process_line(line, &mut control).await.unwrap());
        // now_playing tiene respuesta ok en el fake: éxito.
        assert!(response.get("error").is_none());
        assert!(response["result"]["content"][0]["text"].as_str().unwrap().contains("paused"));
    }

    #[tokio::test]
    async fn tools_call_validates_required_arguments() {
        let mut control = fake_control();
        let line = r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"search","arguments":{}}}"#;
        let response = parse(process_line(line, &mut control).await.unwrap());
        assert!(response["error"]["message"].as_str().unwrap().contains("query"));
    }

    #[tokio::test]
    async fn tools_call_rejects_unknown_tool() {
        let mut control = fake_control();
        let line = r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"hackear","arguments":{}}}"#;
        let response = parse(process_line(line, &mut control).await.unwrap());
        assert!(response["error"]["message"].as_str().unwrap().contains("tool desconocida"));
    }

    #[tokio::test]
    async fn notifications_get_no_response() {
        let mut control = fake_control();
        let line = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        assert!(process_line(line, &mut control).await.is_none());
    }

    #[tokio::test]
    async fn unknown_method_and_malformed_json_are_rpc_errors() {
        let mut control = fake_control();
        let line = r#"{"jsonrpc":"2.0","id":8,"method":"resources/list"}"#;
        let response = parse(process_line(line, &mut control).await.unwrap());
        assert_eq!(response["error"]["code"], -32601);
        let response = parse(process_line("esto no es json", &mut control).await.unwrap());
        assert_eq!(response["error"]["code"], -32700);
    }
}
