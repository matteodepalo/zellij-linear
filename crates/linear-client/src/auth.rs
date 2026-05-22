//! OAuth token persistence + refresh.
//!
//! The token file lives at `~/.config/zellij-linear/auth.json` and is
//! written/read by the native CLI. The wasm plugin never touches it
//! directly (that would require `FullHdAccess`); it shells out to
//! `zellij-linear token` instead.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::http::{HttpClient, HttpError, HttpResponse, HttpVerb};
use crate::{LINEAR_CLIENT_ID, LINEAR_OAUTH_TOKEN};

/// Persisted OAuth state. Forward-compatible: unknown fields are dropped
/// on read and rewritten without them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthFile {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix seconds when `access_token` expires.
    pub expires_at: u64,
    pub scope: String,
    pub user_id: String,
    pub user_email: String,
}

impl AuthFile {
    /// True when the access token is within `skew` seconds of expiry.
    pub fn needs_refresh(&self, skew_secs: u64) -> bool {
        self.expires_at <= now_unix().saturating_add(skew_secs)
    }
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("not logged in (no auth file at {0})")]
    NotLoggedIn(PathBuf),
    #[error("auth file is unreadable: {0}")]
    Io(#[from] std::io::Error),
    #[error("auth file is malformed: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("refresh failed: {0}")]
    Refresh(String),
    #[error("http error: {0}")]
    Http(#[from] HttpError),
}

/// Absolute path of the persisted auth file. The directory is created
/// on first save; reads tolerate the file being absent.
///
/// * Unix:    `$HOME/.config/zellij-linear/auth.json`
/// * Windows: `%APPDATA%\zellij-linear\auth.json`
pub fn auth_file_path() -> PathBuf {
    config_dir().join("auth.json")
}

#[cfg(unix)]
fn config_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("zellij-linear")
}

#[cfg(windows)]
fn config_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("zellij-linear")
}

#[cfg(not(any(unix, windows)))]
fn config_dir() -> PathBuf {
    PathBuf::from(".zellij-linear")
}

pub fn load() -> Result<AuthFile, AuthError> {
    let path = auth_file_path();
    if !path.exists() {
        return Err(AuthError::NotLoggedIn(path));
    }
    let bytes = fs::read(&path)?;
    let auth: AuthFile = serde_json::from_slice(&bytes)?;
    Ok(auth)
}

pub fn save(auth: &AuthFile) -> Result<(), AuthError> {
    let path = auth_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(auth)?;
    fs::write(&path, &json)?;
    // `OpenOptions::mode(0o600)` only applies at file creation. Refreshing
    // tokens rewrites the file repeatedly, so re-assert 0600 on every save.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn delete() -> Result<(), AuthError> {
    let path = auth_file_path();
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

/// OAuth token-exchange / refresh response.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    /// Seconds until expiry.
    pub expires_in: u64,
    pub scope: String,
    #[serde(default)]
    pub token_type: String,
}

/// Exchange an authorization code for tokens (used by `login`).
pub fn exchange_code(
    http: &dyn HttpClient,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<TokenResponse, AuthError> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "authorization_code")
        .append_pair("client_id", LINEAR_CLIENT_ID)
        .append_pair("code", code)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("code_verifier", code_verifier)
        .finish();
    post_token(http, body.as_bytes())
}

/// Refresh tokens via `grant_type=refresh_token`.
pub fn refresh(http: &dyn HttpClient, refresh_token: &str) -> Result<TokenResponse, AuthError> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "refresh_token")
        .append_pair("client_id", LINEAR_CLIENT_ID)
        .append_pair("refresh_token", refresh_token)
        .finish();
    post_token(http, body.as_bytes())
}

fn post_token(http: &dyn HttpClient, body: &[u8]) -> Result<TokenResponse, AuthError> {
    let headers = &[("Content-Type", "application/x-www-form-urlencoded")];
    let resp: HttpResponse = http.request(LINEAR_OAUTH_TOKEN, HttpVerb::Post, headers, body)?;
    if !resp.is_success() {
        return Err(AuthError::Refresh(format!(
            "status {}: {}",
            resp.status,
            resp.body_as_str()
        )));
    }
    let parsed: TokenResponse = serde_json::from_slice(&resp.body)?;
    Ok(parsed)
}

/// Convert a [`TokenResponse`] + caller-provided viewer identity into a
/// persistable [`AuthFile`].
pub fn auth_from_token(token: TokenResponse, user_id: String, user_email: String) -> AuthFile {
    AuthFile {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: now_unix().saturating_add(token.expires_in),
        scope: token.scope,
        user_id,
        user_email,
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
