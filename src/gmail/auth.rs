//! OAuth2 loopback flow + token cache for Gmail.
//!
//! The desktop client opens the user's browser, runs a one-shot HTTP listener
//! on `127.0.0.1:<random>` to catch the redirect, exchanges the code for an
//! access + refresh token, and caches both in `token_cache.json`. Subsequent
//! runs use the refresh token silently.

use anyhow::{anyhow, Context, Result};
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::config::GmailConfig;

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
/// gmail.compose covers drafts.create + drafts.send — the minimum scope set
/// the scheduler needs.
const SCOPE: &str = "https://www.googleapis.com/auth/gmail.compose";

/// On-disk shape of `token_cache.json`. `expires_at_secs` is a UNIX timestamp;
/// we refresh ~60s early to absorb clock skew.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCache {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    pub expires_at_secs: u64,
}

impl TokenCache {
    pub fn load<P: AsRef<Path>>(path: P) -> Option<Self> {
        let s = std::fs::read_to_string(path.as_ref()).ok()?;
        serde_json::from_str(&s).ok()
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let body = serde_json::to_string_pretty(self)?;
        std::fs::write(path.as_ref(), body)
            .with_context(|| format!("writing token cache to {}", path.as_ref().display()))?;
        Ok(())
    }

    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Refresh 60 seconds before the actual expiry to avoid mid-flight failures.
        self.expires_at_secs <= now + 60
    }
}

fn build_client(cfg: &GmailConfig, redirect: &str) -> Result<BasicClient> {
    if cfg.client_id.is_empty() || cfg.client_secret.is_empty() {
        return Err(anyhow!("GMAIL_CLIENT_ID / GMAIL_CLIENT_SECRET missing — fill .env"));
    }
    Ok(BasicClient::new(
        ClientId::new(cfg.client_id.clone()),
        Some(ClientSecret::new(cfg.client_secret.clone())),
        AuthUrl::new(AUTH_URL.to_string())?,
        Some(TokenUrl::new(TOKEN_URL.to_string())?),
    )
    .set_redirect_uri(RedirectUrl::new(redirect.to_string())?))
}

/// Run the one-time browser consent flow. Writes `token_cache.json` and returns
/// the resulting [`TokenCache`].
pub async fn run_setup_flow(cfg: &GmailConfig) -> Result<TokenCache> {
    let listener = TcpListener::bind("127.0.0.1:0").context("binding loopback listener")?;
    let port = listener.local_addr()?.port();
    let redirect = format!("http://127.0.0.1:{port}/callback");

    let client = build_client(cfg, &redirect)?;
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let (auth_url, csrf_state) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new(SCOPE.to_string()))
        .add_extra_param("access_type", "offline")
        .add_extra_param("prompt", "consent")
        .set_pkce_challenge(pkce_challenge)
        .url();

    log::info!("opening browser for Gmail consent: {auth_url}");
    if webbrowser::open(auth_url.as_str()).is_err() {
        eprintln!("Could not open browser automatically. Visit this URL manually:\n{auth_url}");
    }

    // Block for the first incoming request — accept on the same thread so we
    // can hand the verifier straight to the token exchange.
    let (mut stream, _) = listener.accept().context("waiting for OAuth redirect")?;
    let reader = BufReader::new(&mut stream);
    let request_line = reader.lines().next().ok_or_else(|| anyhow!("empty request"))??;
    // request_line = "GET /callback?state=...&code=... HTTP/1.1"
    let path = request_line.split_whitespace().nth(1).unwrap_or("/");
    let (code, state) = parse_callback(path);
    let code = code.ok_or_else(|| anyhow!("missing ?code= in callback"))?;
    let state = state.ok_or_else(|| anyhow!("missing ?state= in callback"))?;
    if state != *csrf_state.secret() {
        return Err(anyhow!("CSRF state mismatch — abort"));
    }

    let response_body = "Hired :: Gmail authentication complete. You can close this tab.";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    let _ = stream.write_all(response.as_bytes());

    let token = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(pkce_verifier)
        .request_async(async_http_client)
        .await
        .context("exchanging code for token")?;

    let access = token.access_token().secret().clone();
    let refresh = token
        .refresh_token()
        .map(|r| r.secret().clone())
        .unwrap_or_default();
    let expires_in = token.expires_in().unwrap_or(Duration::from_secs(3600));
    let cache = TokenCache {
        access_token: access,
        refresh_token: refresh,
        expires_at_secs: unix_now() + expires_in.as_secs(),
    };
    cache.save(&cfg.token_cache)?;
    log::info!("Gmail token saved to {}", cfg.token_cache);
    Ok(cache)
}

/// Refresh `cache.access_token` in place using the stored refresh token, then
/// persist the new expiry.
async fn refresh(cfg: &GmailConfig, cache: &mut TokenCache) -> Result<()> {
    if cache.refresh_token.is_empty() {
        return Err(anyhow!("no refresh_token in cache — re-run setup"));
    }
    // OAuth2 crate doesn't need a redirect URL for refresh, but BasicClient
    // requires one in the constructor — pass a stub.
    let client = build_client(cfg, "http://127.0.0.1/unused")?;
    let token = client
        .exchange_refresh_token(&RefreshToken::new(cache.refresh_token.clone()))
        .request_async(async_http_client)
        .await
        .context("refreshing access token")?;
    cache.access_token = token.access_token().secret().clone();
    if let Some(r) = token.refresh_token() {
        cache.refresh_token = r.secret().clone();
    }
    let expires_in = token.expires_in().unwrap_or(Duration::from_secs(3600));
    cache.expires_at_secs = unix_now() + expires_in.as_secs();
    cache.save(&cfg.token_cache)?;
    Ok(())
}

/// Load the token cache and refresh if it's expired. Errors if no cache exists.
pub async fn ensure_token(cfg: &GmailConfig) -> Result<TokenCache> {
    let mut cache = TokenCache::load(&cfg.token_cache)
        .ok_or_else(|| anyhow!("no token cache at {} — run `scheduler --setup` first", cfg.token_cache))?;
    if cache.is_expired() {
        refresh(cfg, &mut cache).await?;
    }
    Ok(cache)
}

fn parse_callback(path: &str) -> (Option<String>, Option<String>) {
    let Some(qs) = path.split_once('?').map(|x| x.1) else { return (None, None) };
    let mut code = None;
    let mut state = None;
    for pair in qs.split('&') {
        let mut kv = pair.splitn(2, '=');
        let (Some(k), Some(v)) = (kv.next(), kv.next()) else { continue };
        let v = urldecode(v);
        match k {
            "code" => code = Some(v),
            "state" => state = Some(v),
            _ => {}
        }
    }
    (code, state)
}

fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => { out.push(b' '); i += 1; }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    out.push(b);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            other => { out.push(other); i += 1; }
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
