use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::config::OAuthConfig;

/// Persisted OAuth access token bundle used by the CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthTokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
    pub scopes: Vec<String>,
    /// OpenID token from auth.openai.com, needed to extract `chatgpt_account_id`
    /// for the WHAM backend (`ChatGPT-Account-Id` header).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
}

/// PKCE verifier/challenge pair generated for an OAuth authorization flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkceCodePair {
    pub verifier: String,
    pub challenge: String,
    pub challenge_method: PkceChallengeMethod,
}

/// Challenge algorithms supported by the local PKCE helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkceChallengeMethod {
    S256,
}

impl PkceChallengeMethod {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::S256 => "S256",
        }
    }
}

/// Parameters needed to build an authorization URL for browser-based login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthAuthorizationRequest {
    pub authorize_url: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub state: String,
    pub code_challenge: String,
    pub code_challenge_method: PkceChallengeMethod,
    pub extra_params: BTreeMap<String, String>,
}

/// Request body for exchanging an OAuth authorization code for tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthTokenExchangeRequest {
    pub grant_type: &'static str,
    pub code: String,
    pub redirect_uri: String,
    pub client_id: String,
    pub code_verifier: String,
    pub state: String,
}

/// Request body for refreshing an existing OAuth token set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthRefreshRequest {
    pub grant_type: &'static str,
    pub refresh_token: String,
    pub client_id: String,
    pub scopes: Vec<String>,
}

/// Parsed query parameters returned to the local OAuth callback endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthCallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredOAuthCredentials {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_at: Option<u64>,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    id_token: Option<String>,
}

impl From<OAuthTokenSet> for StoredOAuthCredentials {
    fn from(value: OAuthTokenSet) -> Self {
        Self {
            access_token: value.access_token,
            refresh_token: value.refresh_token,
            expires_at: value.expires_at,
            scopes: value.scopes,
            id_token: value.id_token,
        }
    }
}

impl From<StoredOAuthCredentials> for OAuthTokenSet {
    fn from(value: StoredOAuthCredentials) -> Self {
        Self {
            access_token: value.access_token,
            refresh_token: value.refresh_token,
            expires_at: value.expires_at,
            scopes: value.scopes,
            id_token: value.id_token,
        }
    }
}

impl OAuthAuthorizationRequest {
    #[must_use]
    pub fn from_config(
        config: &OAuthConfig,
        redirect_uri: impl Into<String>,
        state: impl Into<String>,
        pkce: &PkceCodePair,
    ) -> Self {
        Self {
            authorize_url: config.authorize_url.clone(),
            client_id: config.client_id.clone(),
            redirect_uri: redirect_uri.into(),
            scopes: config.scopes.clone(),
            state: state.into(),
            code_challenge: pkce.challenge.clone(),
            code_challenge_method: pkce.challenge_method,
            extra_params: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_extra_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_params.insert(key.into(), value.into());
        self
    }

    #[must_use]
    pub fn build_url(&self) -> String {
        let mut params = vec![
            ("response_type", "code".to_string()),
            ("client_id", self.client_id.clone()),
            ("redirect_uri", self.redirect_uri.clone()),
            ("scope", self.scopes.join(" ")),
            ("state", self.state.clone()),
            ("code_challenge", self.code_challenge.clone()),
            (
                "code_challenge_method",
                self.code_challenge_method.as_str().to_string(),
            ),
        ];
        params.extend(
            self.extra_params
                .iter()
                .map(|(key, value)| (key.as_str(), value.clone())),
        );
        let query = params
            .into_iter()
            .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(&value)))
            .collect::<Vec<_>>()
            .join("&");
        format!(
            "{}{}{}",
            self.authorize_url,
            if self.authorize_url.contains('?') {
                '&'
            } else {
                '?'
            },
            query
        )
    }
}

impl OAuthTokenExchangeRequest {
    #[must_use]
    pub fn from_config(
        config: &OAuthConfig,
        code: impl Into<String>,
        state: impl Into<String>,
        verifier: impl Into<String>,
        redirect_uri: impl Into<String>,
    ) -> Self {
        Self {
            grant_type: "authorization_code",
            code: code.into(),
            redirect_uri: redirect_uri.into(),
            client_id: config.client_id.clone(),
            code_verifier: verifier.into(),
            state: state.into(),
        }
    }

    #[must_use]
    pub fn form_params(&self) -> BTreeMap<&str, String> {
        BTreeMap::from([
            ("grant_type", self.grant_type.to_string()),
            ("code", self.code.clone()),
            ("redirect_uri", self.redirect_uri.clone()),
            ("client_id", self.client_id.clone()),
            ("code_verifier", self.code_verifier.clone()),
        ])
    }
}

impl OAuthRefreshRequest {
    #[must_use]
    pub fn from_config(
        config: &OAuthConfig,
        refresh_token: impl Into<String>,
        scopes: Option<Vec<String>>,
    ) -> Self {
        Self {
            grant_type: "refresh_token",
            refresh_token: refresh_token.into(),
            client_id: config.client_id.clone(),
            scopes: scopes.unwrap_or_else(|| config.scopes.clone()),
        }
    }

    #[must_use]
    pub fn form_params(&self) -> BTreeMap<&str, String> {
        BTreeMap::from([
            ("grant_type", self.grant_type.to_string()),
            ("refresh_token", self.refresh_token.clone()),
            ("client_id", self.client_id.clone()),
            ("scope", self.scopes.join(" ")),
        ])
    }
}

pub fn generate_pkce_pair() -> io::Result<PkceCodePair> {
    let verifier = generate_random_token(32)?;
    Ok(PkceCodePair {
        challenge: code_challenge_s256(&verifier),
        verifier,
        challenge_method: PkceChallengeMethod::S256,
    })
}

pub fn generate_state() -> io::Result<String> {
    generate_random_token(32)
}

#[must_use]
pub fn code_challenge_s256(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64url_encode(&digest)
}

#[must_use]
pub fn loopback_redirect_uri(port: u16) -> String {
    format!("http://localhost:{port}/callback")
}

#[must_use]
pub fn loopback_redirect_uri_with_path(port: u16, path: &str) -> String {
    format!("http://localhost:{port}{path}")
}

pub fn credentials_path() -> io::Result<PathBuf> {
    Ok(credentials_home_dir()?.join("credentials.json"))
}

pub fn load_oauth_credentials() -> io::Result<Option<OAuthTokenSet>> {
    let path = credentials_path()?;
    let root = read_credentials_root(&path)?;
    let Some(oauth) = root.get("oauth") else {
        return Ok(None);
    };
    if oauth.is_null() {
        return Ok(None);
    }
    let stored = serde_json::from_value::<StoredOAuthCredentials>(oauth.clone())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(Some(stored.into()))
}

pub fn save_oauth_credentials(token_set: &OAuthTokenSet) -> io::Result<()> {
    let path = credentials_path()?;
    let mut root = read_credentials_root(&path)?;
    root.insert(
        "oauth".to_string(),
        serde_json::to_value(StoredOAuthCredentials::from(token_set.clone()))
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
    );
    write_credentials_root(&path, &root)
}

pub fn clear_oauth_credentials() -> io::Result<()> {
    let path = credentials_path()?;
    let mut root = read_credentials_root(&path)?;
    root.remove("oauth");
    write_credentials_root(&path, &root)
}

// ---------------------------------------------------------------------------
// Per-provider OAuth token storage
// ---------------------------------------------------------------------------

/// Load OAuth credentials for a specific provider from `credentials.json`.
/// Credentials are stored under the `oauth_providers.{provider_id}` key.
pub fn load_provider_oauth(provider_id: &str) -> io::Result<Option<OAuthTokenSet>> {
    let path = credentials_path()?;
    let root = read_credentials_root(&path)?;
    let Some(oauth_providers) = root.get("oauth_providers") else {
        return Ok(None);
    };
    let Some(provider_value) = oauth_providers.get(provider_id) else {
        return Ok(None);
    };
    if provider_value.is_null() {
        return Ok(None);
    }
    let stored = serde_json::from_value::<StoredOAuthCredentials>(provider_value.clone())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(Some(stored.into()))
}

/// Save OAuth credentials for a specific provider to `credentials.json`.
/// Preserves other providers and the legacy `"oauth"` key.
pub fn save_provider_oauth(provider_id: &str, token_set: &OAuthTokenSet) -> io::Result<()> {
    let path = credentials_path()?;
    let mut root = read_credentials_root(&path)?;
    let oauth_providers = root
        .entry("oauth_providers")
        .or_insert_with(|| Value::Object(Map::new()));
    let provider_map = oauth_providers
        .as_object_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "oauth_providers must be an object"))?;
    provider_map.insert(
        provider_id.to_string(),
        serde_json::to_value(StoredOAuthCredentials::from(token_set.clone()))
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
    );
    write_credentials_root(&path, &root)
}

/// Clear OAuth credentials for a specific provider.
pub fn clear_provider_oauth(provider_id: &str) -> io::Result<()> {
    let path = credentials_path()?;
    let mut root = read_credentials_root(&path)?;
    let Some(oauth_providers) = root.get_mut("oauth_providers") else {
        return Ok(());
    };
    let Some(provider_map) = oauth_providers.as_object_mut() else {
        return Ok(());
    };
    provider_map.remove(provider_id);
    if provider_map.is_empty() {
        root.remove("oauth_providers");
    }
    write_credentials_root(&path, &root)
}

// ---------------------------------------------------------------------------
// Browser launcher
// ---------------------------------------------------------------------------

/// Open a URL in the user's default browser.
/// Falls back to printing the URL if the platform command fails.
pub fn open_browser(url: &str) -> io::Result<()> {
    let (cmd, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(target_os = "linux") {
        ("xdg-open", vec![url])
    } else if cfg!(target_os = "windows") {
        ("cmd", vec!["/C", "start", "", url])
    } else {
        eprintln!("Please open this URL in your browser:");
        eprintln!("  {url}");
        return Ok(());
    };
    match std::process::Command::new(cmd).args(&args).output() {
        Ok(output) if output.status.success() => Ok(()),
        _ => {
            eprintln!("Could not open browser automatically. Please open this URL:");
            eprintln!("  {url}");
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Local HTTP callback server (blocking, single-request)
// ---------------------------------------------------------------------------

/// Result of a successful OAuth callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthCallbackResult {
    pub code: String,
    pub state: String,
}

/// Run a blocking local HTTP server that waits for a single callback request.
/// Returns the authorization code and state on success.
/// Times out after `timeout` duration.
pub fn run_oauth_callback_server(
    port: u16,
    timeout: std::time::Duration,
    callback_path: &str,
) -> io::Result<OAuthCallbackResult> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::{SocketAddr, TcpListener};
    use std::sync::mpsc;

    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().map_err(|e| {
        io::Error::new(io::ErrorKind::InvalidInput, format!("invalid address: {e}"))
    })?;
    let listener = TcpListener::bind(addr)?;

    let (tx, rx) = mpsc::channel::<std::net::TcpStream>();

    std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            let _ = tx.send(stream);
        }
    });

    let mut stream = match rx.recv_timeout(timeout) {
        Ok(stream) => stream,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "OAuth callback timed out waiting for browser redirect",
            ));
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "callback server thread disconnected",
            ));
        }
    };

    let mut reader = BufReader::new(&mut stream);
    let mut first_line = String::new();
    reader.read_line(&mut first_line)?;

    // Parse "GET /callback?code=...&state=... HTTP/1.1"
    let target = first_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("");

    // Consume remaining headers so browser doesn't reset connection
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
    }

    match parse_oauth_callback_request_target(target, callback_path) {
        Ok(params) => {
            if let (Some(code), Some(state)) = (&params.code, &params.state) {
                // Success page
                let body = r#"<!DOCTYPE html>
<html><head><title>Authentication Successful</title></head>
<body style="font-family:sans-serif;text-align:center;padding:40px;">
<h1>✅ Authentication Successful</h1>
<p>You can close this tab and return to the terminal.</p>
</body></html>"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes())?;
                return Ok(OAuthCallbackResult {
                    code: code.clone(),
                    state: state.clone(),
                });
            }
            if let Some(error) = &params.error {
                let err_desc = params.error_description.as_deref().unwrap_or(error);
                let body = format!(
                    r#"<!DOCTYPE html>
<html><head><title>Authentication Failed</title></head>
<body style="font-family:sans-serif;text-align:center;padding:40px;">
<h1>❌ Authentication Failed</h1>
<p>{}</p>
<p>You can close this tab and return to the terminal.</p>
</body></html>"#,
                    html_escape(err_desc)
                );
                let response = format!(
                    "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes())?;
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("OAuth error: {error} - {err_desc}"),
                ));
            }
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "callback received neither code nor error",
            ))
        }
        Err(e) => {
            let body = format!(
                r#"<!DOCTYPE html>
<html><head><title>Authentication Failed</title></head>
<body style="font-family:sans-serif;text-align:center;padding:40px;">
<h1>❌ Invalid Callback</h1>
<p>{}</p>
</body></html>"#,
                html_escape(&e)
            );
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes())?;
            Err(io::Error::new(io::ErrorKind::Other, e))
        }
    }
}

#[must_use]
fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// Device Authorization Flow (RFC 8628)
// ---------------------------------------------------------------------------

/// Request body for starting a device authorization flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceAuthRequest {
    pub client_id: String,
    pub scope: String,
}

/// Response from a device authorization endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DeviceAuthResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    pub interval: u64,
}

/// Poll a device token endpoint until the user authorizes or the flow expires.
/// Returns `Ok(None)` if the user hasn't authorized yet but we should keep polling.
/// Returns `Ok(Some(token_set))` on success.
/// Returns `Err` on fatal errors.
pub async fn poll_device_token(
    client: &reqwest::Client,
    device_code: &str,
    client_id: &str,
    token_url: &str,
) -> io::Result<Option<OAuthTokenSet>> {
    let params = [
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ("device_code", device_code),
        ("client_id", client_id),
    ];

    let response = client
        .post(token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("HTTP request failed: {e}")))?;
    let status = response.status();
    let body = response.text().await.map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("Failed to read response body: {e}"))
    })?;

    if status.is_success() {
        let token: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let access_token = token["access_token"]
            .as_str()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "access_token missing from device token response",
                )
            })?
            .to_string();
        let refresh_token = token["refresh_token"].as_str().map(String::from);
        let expires_at = token["expires_in"].as_u64().map(|secs| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                + secs
        });
        let scopes = token["scope"]
            .as_str()
            .map(|s| s.split(' ').map(String::from).collect())
            .unwrap_or_default();
        let id_token = token["id_token"].as_str().map(String::from);
        return Ok(Some(OAuthTokenSet {
            access_token,
            refresh_token,
            expires_at,
            scopes,
            id_token,
        }));
    }

    // Parse OAuth error response
    let error_json: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    let error = error_json["error"].as_str().unwrap_or("unknown");

    match error {
        "authorization_pending" => Ok(None),
        "slow_down" => {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            Ok(None)
        }
        "expired_token" => Err(io::Error::new(
            io::ErrorKind::Other,
            "Device authorization expired. Please try again.",
        )),
        "access_denied" => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "User denied authorization.",
        )),
        _ => Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Device token error: {error}: {body}"),
        )),
    }
}

pub fn parse_oauth_callback_request_target(
    target: &str,
    expected_path: &str,
) -> Result<OAuthCallbackParams, String> {
    let (path, query) = target
        .split_once('?')
        .map_or((target, ""), |(path, query)| (path, query));
    if path != expected_path {
        return Err(format!("unexpected callback path: {path}, expected: {expected_path}"));
    }
    parse_oauth_callback_query(query)
}

pub fn parse_oauth_callback_query(query: &str) -> Result<OAuthCallbackParams, String> {
    let mut params = BTreeMap::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair
            .split_once('=')
            .map_or((pair, ""), |(key, value)| (key, value));
        params.insert(percent_decode(key)?, percent_decode(value)?);
    }
    Ok(OAuthCallbackParams {
        code: params.get("code").cloned(),
        state: params.get("state").cloned(),
        error: params.get("error").cloned(),
        error_description: params.get("error_description").cloned(),
    })
}

fn generate_random_token(bytes: usize) -> io::Result<String> {
    let mut buffer = vec![0_u8; bytes];
    File::open("/dev/urandom")?.read_exact(&mut buffer)?;
    Ok(base64url_encode(&buffer))
}

fn credentials_home_dir() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("CLAW_CONFIG_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "HOME is not set (on Windows, set USERPROFILE or HOME, \
                 or use CLAW_CONFIG_HOME to point directly at the config directory)",
            )
        })?;
    Ok(PathBuf::from(home).join(".claw"))
}

fn read_credentials_root(path: &PathBuf) -> io::Result<Map<String, Value>> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            if contents.trim().is_empty() {
                return Ok(Map::new());
            }
            serde_json::from_str::<Value>(&contents)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
                .as_object()
                .cloned()
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "credentials file must contain a JSON object",
                    )
                })
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Map::new()),
        Err(error) => Err(error),
    }
}

fn write_credentials_root(path: &PathBuf, root: &Map<String, Value>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let rendered = serde_json::to_string_pretty(&Value::Object(root.clone()))
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, format!("{rendered}\n"))?;
    fs::rename(temp_path, path)
}

fn base64url_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::new();
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let block = (u32::from(bytes[index]) << 16)
            | (u32::from(bytes[index + 1]) << 8)
            | u32::from(bytes[index + 2]);
        output.push(TABLE[((block >> 18) & 0x3F) as usize] as char);
        output.push(TABLE[((block >> 12) & 0x3F) as usize] as char);
        output.push(TABLE[((block >> 6) & 0x3F) as usize] as char);
        output.push(TABLE[(block & 0x3F) as usize] as char);
        index += 3;
    }
    match bytes.len().saturating_sub(index) {
        1 => {
            let block = u32::from(bytes[index]) << 16;
            output.push(TABLE[((block >> 18) & 0x3F) as usize] as char);
            output.push(TABLE[((block >> 12) & 0x3F) as usize] as char);
        }
        2 => {
            let block = (u32::from(bytes[index]) << 16) | (u32::from(bytes[index + 1]) << 8);
            output.push(TABLE[((block >> 18) & 0x3F) as usize] as char);
            output.push(TABLE[((block >> 12) & 0x3F) as usize] as char);
            output.push(TABLE[((block >> 6) & 0x3F) as usize] as char);
        }
        _ => {}
    }
    output
}

/// Extract `chatgpt_account_id` from a JWT payload (id_token or access_token).
/// No signature verification — just base64-decode the payload section.
pub fn extract_chatgpt_account_id(token: &str) -> Option<String> {
    let payload_b64 = token.split('.').nth(1)?;
    let payload_json = base64_decode_urlsafe(payload_b64).ok()?;
    let payload: serde_json::Value = serde_json::from_slice(&payload_json).ok()?;

    // 3-level fallback, matching Codex CLI behaviour:
    // 1. top-level `chatgpt_account_id`
    if let Some(id) = payload.get("chatgpt_account_id").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }
    // 2. `https://api.openai.com/auth` -> `chatgpt_account_id`
    if let Some(auth) = payload.get("https://api.openai.com/auth").and_then(|v| v.as_object()) {
        if let Some(id) = auth.get("chatgpt_account_id").and_then(|v| v.as_str()) {
            return Some(id.to_string());
        }
    }
    // 3. `organizations[0].id`
    if let Some(orgs) = payload.get("organizations").and_then(|v| v.as_array()) {
        if let Some(first) = orgs.first() {
            if let Some(id) = first.get("id").and_then(|v| v.as_str()) {
                return Some(id.to_string());
            }
        }
    }
    None
}

fn base64_decode_urlsafe(input: &str) -> Result<Vec<u8>, String> {
    let padded = match input.len() % 4 {
        0 => input.to_string(),
        2 => format!("{input}=="),
        3 => format!("{input}="),
        _ => return Err("invalid base64url length".to_string()),
    };
    let standard = padded.replace('-', "+").replace('_', "/");
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &standard)
        .map_err(|e| e.to_string())
}

/// Refresh an OAuth token set using the refresh_token grant.
/// Returns a new token set on success.
pub async fn refresh_oauth_token(
    client: &reqwest::Client,
    token_url: &str,
    client_id: &str,
    refresh_token: &str,
) -> io::Result<OAuthTokenSet> {
    let params = [
        ("grant_type", "refresh_token"),
        ("client_id", client_id),
        ("refresh_token", refresh_token),
    ];

    let response = client
        .post(token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("HTTP request failed: {e}")))?;

    let status = response.status();
    let body = response.text().await.map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("Failed to read response body: {e}"))
    })?;

    if !status.is_success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Token refresh failed ({status}): {body}"),
        ));
    }

    let token: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let access_token = token["access_token"]
        .as_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "access_token missing from refresh response"))?
        .to_string();
    let refresh_token = token["refresh_token"].as_str().map(String::from);
    let id_token = token["id_token"].as_str().map(String::from);
    let expires_at = token["expires_in"].as_u64().map(|secs| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + secs
    });
    let scopes = token["scope"]
        .as_str()
        .map(|s| s.split(' ').map(String::from).collect())
        .unwrap_or_default();

    Ok(OAuthTokenSet {
        access_token,
        refresh_token,
        expires_at,
        scopes,
        id_token,
    })
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(&mut encoded, "%{byte:02X}");
            }
        }
    }
    encoded
}

fn percent_decode(value: &str) -> Result<String, String> {
    let mut decoded = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hi = decode_hex(bytes[index + 1])?;
                let lo = decode_hex(bytes[index + 2])?;
                decoded.push((hi << 4) | lo);
                index += 3;
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).map_err(|error| error.to_string())
}

fn decode_hex(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("invalid percent byte: {byte}")),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        clear_oauth_credentials, clear_provider_oauth, code_challenge_s256, credentials_path,
        generate_pkce_pair, generate_state, load_oauth_credentials, load_provider_oauth,
        loopback_redirect_uri, parse_oauth_callback_query, parse_oauth_callback_request_target,
        run_oauth_callback_server, save_oauth_credentials, save_provider_oauth, html_escape,
        OAuthAuthorizationRequest, OAuthConfig, OAuthRefreshRequest, OAuthTokenExchangeRequest,
        OAuthTokenSet,
    };

    fn sample_config() -> OAuthConfig {
        OAuthConfig {
            client_id: "runtime-client".to_string(),
            authorize_url: "https://console.test/oauth/authorize".to_string(),
            token_url: "https://console.test/oauth/token".to_string(),
            callback_port: Some(4545),
            manual_redirect_url: Some("https://console.test/oauth/callback".to_string()),
            scopes: vec!["org:read".to_string(), "user:write".to_string()],
        }
    }

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_env_lock()
    }

    fn temp_config_home() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "runtime-oauth-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
    }

    #[test]
    fn s256_challenge_matches_expected_vector() {
        assert_eq!(
            code_challenge_s256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn generates_pkce_pair_and_state() {
        let pair = generate_pkce_pair().expect("pkce pair");
        let state = generate_state().expect("state");
        assert!(!pair.verifier.is_empty());
        assert!(!pair.challenge.is_empty());
        assert!(!state.is_empty());
    }

    #[test]
    fn builds_authorize_url_and_form_requests() {
        let config = sample_config();
        let pair = generate_pkce_pair().expect("pkce");
        let url = OAuthAuthorizationRequest::from_config(
            &config,
            loopback_redirect_uri(4545),
            "state-123",
            &pair,
        )
        .with_extra_param("login_hint", "user@example.com")
        .build_url();
        assert!(url.starts_with("https://console.test/oauth/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=runtime-client"));
        assert!(url.contains("scope=org%3Aread%20user%3Awrite"));
        assert!(url.contains("login_hint=user%40example.com"));

        let exchange = OAuthTokenExchangeRequest::from_config(
            &config,
            "auth-code",
            "state-123",
            pair.verifier,
            loopback_redirect_uri(4545),
        );
        assert_eq!(
            exchange.form_params().get("grant_type").map(String::as_str),
            Some("authorization_code")
        );

        let refresh = OAuthRefreshRequest::from_config(&config, "refresh-token", None);
        assert_eq!(
            refresh.form_params().get("scope").map(String::as_str),
            Some("org:read user:write")
        );
    }

    #[test]
    fn oauth_credentials_round_trip_and_clear_preserves_other_fields() {
        let _guard = env_lock();
        let config_home = temp_config_home();
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        let path = credentials_path().expect("credentials path");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        std::fs::write(&path, "{\"other\":\"value\"}\n").expect("seed credentials");

        let token_set = OAuthTokenSet {
            access_token: "access-token".to_string(),
            refresh_token: Some("refresh-token".to_string()),
            expires_at: Some(123),
            scopes: vec!["scope:a".to_string()],
            id_token: None,
        };
        save_oauth_credentials(&token_set).expect("save credentials");
        assert_eq!(
            load_oauth_credentials().expect("load credentials"),
            Some(token_set)
        );
        let saved = std::fs::read_to_string(&path).expect("read saved file");
        assert!(saved.contains("\"other\": \"value\""));
        assert!(saved.contains("\"oauth\""));

        clear_oauth_credentials().expect("clear credentials");
        assert_eq!(load_oauth_credentials().expect("load cleared"), None);
        let cleared = std::fs::read_to_string(&path).expect("read cleared file");
        assert!(cleared.contains("\"other\": \"value\""));
        assert!(!cleared.contains("\"oauth\""));

        std::env::remove_var("CLAW_CONFIG_HOME");
        std::fs::remove_dir_all(config_home).expect("cleanup temp dir");
    }

    #[test]
    fn parses_callback_query_and_target() {
        let params =
            parse_oauth_callback_query("code=abc123&state=state-1&error_description=needs%20login")
                .expect("parse query");
        assert_eq!(params.code.as_deref(), Some("abc123"));
        assert_eq!(params.state.as_deref(), Some("state-1"));
        assert_eq!(params.error_description.as_deref(), Some("needs login"));

        let params = parse_oauth_callback_request_target("/callback?code=abc&state=xyz", "/callback")
            .expect("parse callback target");
        assert_eq!(params.code.as_deref(), Some("abc"));
        assert_eq!(params.state.as_deref(), Some("xyz"));
        assert!(parse_oauth_callback_request_target("/wrong?code=abc", "/callback").is_err());
    }

    #[test]
    fn provider_oauth_credentials_round_trip_and_clear() {
        let _guard = env_lock();
        let config_home = temp_config_home();
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        let path = credentials_path().expect("credentials path");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");

        let openai_tokens = OAuthTokenSet {
            access_token: "openai-access".to_string(),
            refresh_token: Some("openai-refresh".to_string()),
            expires_at: Some(1000),
            scopes: vec!["openid".to_string()],
            id_token: None,
        };
        let moonshot_tokens = OAuthTokenSet {
            access_token: "moonshot-access".to_string(),
            refresh_token: None,
            expires_at: Some(2000),
            scopes: vec!["profile".to_string()],
            id_token: None,
        };

        save_provider_oauth("openai", &openai_tokens).expect("save openai");
        save_provider_oauth("moonshot", &moonshot_tokens).expect("save moonshot");

        assert_eq!(
            load_provider_oauth("openai").expect("load openai"),
            Some(openai_tokens.clone())
        );
        assert_eq!(
            load_provider_oauth("moonshot").expect("load moonshot"),
            Some(moonshot_tokens.clone())
        );
        assert_eq!(
            load_provider_oauth("unknown").expect("load unknown"),
            None
        );

        let saved = std::fs::read_to_string(&path).expect("read saved file");
        assert!(saved.contains("\"oauth_providers\""));
        assert!(saved.contains("\"openai\""));
        assert!(saved.contains("\"moonshot\""));

        clear_provider_oauth("openai").expect("clear openai");
        assert_eq!(load_provider_oauth("openai").expect("load cleared"), None);
        assert_eq!(
            load_provider_oauth("moonshot").expect("load moonshot after clear"),
            Some(moonshot_tokens)
        );

        clear_provider_oauth("moonshot").expect("clear moonshot");
        let cleared = std::fs::read_to_string(&path).expect("read cleared file");
        assert!(!cleared.contains("\"oauth_providers\""));

        std::env::remove_var("CLAW_CONFIG_HOME");
        std::fs::remove_dir_all(config_home).expect("cleanup temp dir");
    }

    #[test]
    fn provider_oauth_preserves_legacy_oauth_key() {
        let _guard = env_lock();
        let config_home = temp_config_home();
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        let path = credentials_path().expect("credentials path");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");

        let legacy = OAuthTokenSet {
            access_token: "legacy-access".to_string(),
            refresh_token: Some("legacy-refresh".to_string()),
            expires_at: Some(999),
            scopes: vec!["org:read".to_string()],
            id_token: None,
        };
        save_oauth_credentials(&legacy).expect("save legacy");

        let provider = OAuthTokenSet {
            access_token: "provider-access".to_string(),
            refresh_token: None,
            expires_at: Some(888),
            scopes: vec!["user:read".to_string()],
            id_token: None,
        };
        save_provider_oauth("openai", &provider).expect("save provider");

        assert_eq!(
            load_oauth_credentials().expect("load legacy"),
            Some(legacy)
        );
        assert_eq!(
            load_provider_oauth("openai").expect("load provider"),
            Some(provider)
        );

        std::env::remove_var("CLAW_CONFIG_HOME");
        std::fs::remove_dir_all(config_home).expect("cleanup temp dir");
    }

    #[test]
    fn callback_server_returns_code_and_state() {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        use std::thread;

        let port = 4547;
        let server_thread = thread::spawn(move || {
            run_oauth_callback_server(port, std::time::Duration::from_secs(5), "/callback")
        });

        // Give the server a moment to bind
        thread::sleep(std::time::Duration::from_millis(100));

        // Simulate browser callback
        let request = format!(
            "GET /callback?code=test-code-123&state=test-state-456 HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n"
        );
        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).expect("connect to callback server");
        stream.write_all(request.as_bytes()).expect("send request");
        stream.flush().expect("flush");

        // Read response (should be HTML success page)
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");
        assert!(response.contains("200 OK"), "expected 200 OK, got: {response}");
        assert!(response.contains("Authentication Successful"), "expected success page");

        let result = server_thread.join().expect("server thread join");
        let callback = result.expect("callback result");
        assert_eq!(callback.code, "test-code-123");
        assert_eq!(callback.state, "test-state-456");
    }

    #[test]
    fn callback_server_returns_error_on_oauth_error() {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        use std::thread;

        let port = 4548;
        let server_thread = thread::spawn(move || {
            run_oauth_callback_server(port, std::time::Duration::from_secs(5), "/callback")
        });

        thread::sleep(std::time::Duration::from_millis(100));

        let request = format!(
            "GET /callback?error=access_denied&error_description=user%20denied HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n"
        );
        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).expect("connect");
        stream.write_all(request.as_bytes()).expect("send");
        stream.flush().expect("flush");

        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read");
        assert!(response.contains("400 Bad Request"), "expected 400, got: {response}");

        let result = server_thread.join().expect("join");
        assert!(result.is_err(), "expected error for OAuth error response");
    }

    #[test]
    fn callback_server_times_out_when_no_request() {
        let port = 4549;
        let result = run_oauth_callback_server(port, std::time::Duration::from_millis(50), "/callback");
        assert!(result.is_err(), "expected timeout error");
    }

    #[test]
    fn html_escape_works() {
        assert_eq!(html_escape("<script>alert('xss')</script>"), "&lt;script&gt;alert('xss')&lt;/script&gt;");
        assert_eq!(html_escape("foo & bar"), "foo &amp; bar");
        assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
    }
}
