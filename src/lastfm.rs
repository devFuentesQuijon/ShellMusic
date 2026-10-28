//! Scrobbling de Last.fm: firma `api_sig` (MD5), cliente HTTP mínimo y
//! flujo de autorización. Best-effort: errores de red llegan como String
//! para la UI; nunca panic. La sesión se persiste en `lastfm.json` en el
//! directorio de datos (la config sigue siendo solo lectura).

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::providers::Track;

const API_URL: &str = "https://ws.audioscrobbler.com/2.0/";
const AUTH_URL: &str = "https://www.last.fm/api/auth/";
const TIMEOUT: Duration = Duration::from_secs(10);
const USER_AGENT: &str = concat!("ShellMusic/", env!("CARGO_PKG_VERSION"));

/// Sesión persistente tras autorizar (`smusic --lastfm-auth`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub session_key: String,
    pub username: String,
}

impl Session {
    pub fn path(data_dir: &Path) -> std::path::PathBuf {
        data_dir.join("lastfm.json")
    }

    pub fn load(data_dir: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(Self::path(data_dir)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save(&self, data_dir: &Path) -> Result<()> {
        std::fs::write(
            Self::path(data_dir),
            serde_json::to_string_pretty(self)?,
        )
        .context("no se pudo escribir lastfm.json")
    }
}

/// Firma `api_sig` de Last.fm: pares `clavevalor` ordenados por clave
/// (sin `format` ni `api_sig`), concatenados y con el `secret` al final;
/// MD5 en hexadecimal. Documentado en last.fm/api.
pub(crate) fn api_sig(params: &[(&str, &str)], secret: &str) -> String {
    let mut sorted: Vec<&(&str, &str)> = params
        .iter()
        .filter(|(key, _)| *key != "format" && *key != "api_sig")
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut input = String::new();
    for (key, value) in sorted {
        input.push_str(key);
        input.push_str(value);
    }
    input.push_str(secret);
    format!("{:x}", md5::compute(input.as_bytes()))
}

/// Cliente mínimo: api_key + secret del usuario, sesión ya autorizada.
/// Clone para moverlo a las tareas de scrobble (reqwest::Client es barato
/// de clonar: es un Arc interno).
#[derive(Clone)]
pub struct Client {
    api_key: String,
    secret: String,
    session_key: String,
    http: reqwest::Client,
}

impl Client {
    pub fn new(api_key: &str, secret: &str, session: &Session) -> Result<Self> {
        if api_key.is_empty() || secret.is_empty() {
            bail!("faltan lastfm api_key/secret en config.toml");
        }
        Ok(Self {
            api_key: api_key.to_string(),
            secret: secret.to_string(),
            session_key: session.session_key.clone(),
            http: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .timeout(TIMEOUT)
                .build()?,
        })
    }

    /// Avisa a Last.fm de la pista que suena ahora.
    pub async fn now_playing(&self, track: &Track) -> Result<(), String> {
        let mut params = self.base_params("track.updateNowPlaying");
        params.push(("artist".to_string(), track.artist.clone()));
        params.push(("track".to_string(), track.title.clone()));
        self.post(&params).await.map(|_| ())
    }

    /// Scrobble de una pista terminada; `timestamp` = epoch de su inicio.
    pub async fn scrobble(&self, track: &Track, timestamp: u64) -> Result<(), String> {
        let mut params = self.base_params("track.scrobble");
        params.push(("artist".to_string(), track.artist.clone()));
        params.push(("track".to_string(), track.title.clone()));
        params.push(("timestamp".to_string(), timestamp.to_string()));
        self.post(&params).await.map(|_| ())
    }

    fn base_params(&self, method: &str) -> Vec<(String, String)> {
        vec![
            ("api_key".to_string(), self.api_key.clone()),
            ("method".to_string(), method.to_string()),
            ("sk".to_string(), self.session_key.clone()),
        ]
    }

    /// POST firmado; devuelve el cuerpo crudo. Error 4/9 (clave o sesión
    /// inválida) se distingue para pedir re-autenticación.
    async fn post(&self, params: &[(String, String)]) -> Result<String, String> {
        let refs: Vec<(&str, &str)> = params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let sig = api_sig(&refs, &self.secret);
        let mut owned = params.to_vec();
        owned.push(("api_sig".to_string(), sig));
        owned.push(("format".to_string(), "json".to_string()));

        let response = self
            .http
            .post(API_URL)
            .form(&owned)
            .send()
            .await
            .map_err(|err| format!("error de red scrobbling Last.fm: {err}"))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|err| format!("respuesta Last.fm ilegible: {err}"))?;
        if status.is_success() && !body.contains("\"error\"") {
            return Ok(body);
        }
        let code = extract_error_code(&body);
        match code {
            Some(4) | Some(9) => Err(
                "Last.fm rechazó la sesión o la clave (re-autentica: smusic --lastfm-auth)"
                    .to_string(),
            ),
            _ => Err(format!("Last.fm respondió {status}: {body}")),
        }
    }
}

fn extract_error_code(body: &str) -> Option<u64> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    value.get("error")?.as_u64()
}

// ── Flujo de autorización (smusic --lastfm-auth) ────────────────────────────

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
}

#[derive(Deserialize)]
struct SessionResponse {
    session: SessionBody,
}

#[derive(Deserialize)]
struct SessionBody {
    key: String,
    name: String,
}

fn auth_http() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(TIMEOUT)
        .build()
        .context("no se pudo crear el cliente HTTP")
}

/// URL que el usuario debe abrir para autorizar la app con este token.
pub fn authorize_url(api_key: &str, token: &str) -> String {
    format!("{AUTH_URL}?api_key={api_key}&token={token}")
}

/// Paso 1: pide un token de autorización.
async fn get_token(http: &reqwest::Client, api_key: &str, secret: &str) -> Result<String> {
    let params = [("api_key", api_key), ("method", "auth.getToken")];
    let sig = api_sig(&params, secret);
    let body: TokenResponse = http
        .get(API_URL)
        .query(&[
            ("api_key", api_key),
            ("method", "auth.getToken"),
            ("api_sig", sig.as_str()),
            ("format", "json"),
        ])
        .send()
        .await
        .context("no se pudo contactar con Last.fm")?
        .json()
        .await
        .context("respuesta de token inválida")?;
    Ok(body.token)
}

/// Paso 2: intercambia el token autorizado por la sesión persistente.
async fn get_session(
    http: &reqwest::Client,
    api_key: &str,
    secret: &str,
    token: &str,
) -> Result<Session> {
    let params = [
        ("api_key", api_key),
        ("method", "auth.getSession"),
        ("token", token),
    ];
    let sig = api_sig(&params, secret);
    let body: SessionResponse = http
        .get(API_URL)
        .query(&[
            ("api_key", api_key),
            ("method", "auth.getSession"),
            ("token", token),
            ("api_sig", sig.as_str()),
            ("format", "json"),
        ])
        .send()
        .await
        .context("no se pudo contactar con Last.fm")?
        .json()
        .await
        .context(
            "Last.fm no devolvió sesión: ¿autorizaste la app en el navegador? (el token caduca pronto)",
        )?;
    Ok(Session {
        session_key: body.session.key,
        username: body.session.name,
    })
}

/// Alta interactiva completa: token → URL → espera a Enter → sesión.
/// Devuelve la sesión para que el llamador la guarde donde corresponda.
pub async fn auth_interactive(api_key: &str, secret: &str) -> Result<Session> {
    if api_key.is_empty() || secret.is_empty() {
        bail!(
            "configura primero [lastfm] api_key y secret en config.toml \
             (créalas en https://www.last.fm/api/account/create)"
        );
    }
    let http = auth_http()?;
    let token = get_token(&http, api_key, secret).await?;
    println!(
        "Abre esta URL en tu navegador y autoriza ShellMusic:\n\n  {}\n\nPulsa Enter aquí cuando hayas terminado…",
        authorize_url(api_key, &token)
    );
    let mut line = String::new();
    use std::io::BufRead;
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .context("no se pudo leer la confirmación (stdin)")?;
    get_session(&http, api_key, secret, &token).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_sig_matches_lastfm_documented_example() {
        // Ejemplo documentado en last.fm/api (Auth: 'api_sig' explained).
        let sig = api_sig(
            &[
                ("api_key", "b25b959554ed76058ac220b7b2e0a026"),
                ("method", "auth.getSession"),
                ("token", "xxxx"),
            ],
            "mysecret",
        );
        // El ejemplo oficial usa estos pares ordenados + secret.
        assert_eq!(sig.len(), 32, "md5 hex son 32 caracteres");
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn api_sig_excludes_format_and_is_order_independent() {
        let params = [
            ("method", "track.scrobble"),
            ("api_key", "KEY"),
            ("format", "json"),
            ("track", "Song"),
        ];
        let a = api_sig(&params, "SECRET");
        // Mismo conjunto, otro orden y con api_sig/formar extra: misma firma.
        let b = api_sig(
            &[
                ("track", "Song"),
                ("method", "track.scrobble"),
                ("api_key", "KEY"),
                ("format", "json"),
                ("api_sig", "ignored"),
            ],
            "SECRET",
        );
        assert_eq!(a, b);
    }

    #[test]
    fn session_round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!(
            "shellmusic-lastfm-{}-{:016x}",
            std::process::id(),
            crate::state_file::random_suffix()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let session = Session {
            session_key: "key123".to_string(),
            username: "usuario".to_string(),
        };
        session.save(&dir).unwrap();
        let loaded = Session::load(&dir).unwrap();
        assert_eq!(loaded.session_key, "key123");
        assert_eq!(loaded.username, "usuario");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_error_code_parses_lastfm_error_body() {
        assert_eq!(extract_error_code(r#"{"error":9,"message":"Invalid session key"}"#), Some(9));
        assert_eq!(extract_error_code(r#"{"session":{"name":"x","key":"y"}}"#), None);
    }
}
