//! Gmail draft → sent. Wraps `drafts.send` with classification so the
//! scheduler can apply the right retry policy.

use thiserror::Error;

const DRAFTS_SEND_URL_TMPL: &str = "https://gmail.googleapis.com/gmail/v1/users/me/drafts/send";

/// Classified send-failure modes — drives the scheduler's retry policy.
#[derive(Debug, Error)]
pub enum SendError {
    #[error("Gmail rate limit hit (HTTP 429): {0}")]
    RateLimit(String),
    #[error("auth rejected (HTTP {0}): {1}")]
    Auth(u16, String),
    #[error("Gmail API error (HTTP {0}): {1}")]
    Api(u16, String),
    #[error("network error: {0}")]
    Network(String),
}

/// Send a draft by its Gmail ID. On success returns `Ok(())`; the caller is
/// responsible for moving the draft into `sent_log.json`.
pub async fn send_draft(access_token: &str, draft_id: &str) -> Result<(), SendError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| SendError::Network(e.to_string()))?;

    let payload = serde_json::json!({ "id": draft_id });
    let resp = client
        .post(DRAFTS_SEND_URL_TMPL)
        .bearer_auth(access_token)
        .json(&payload)
        .send()
        .await
        .map_err(|e| SendError::Network(e.to_string()))?;

    let status = resp.status();
    if status.is_success() {
        log::info!("draft {draft_id} sent");
        return Ok(());
    }
    let code = status.as_u16();
    let body = resp.text().await.unwrap_or_default();
    Err(match code {
        429 => SendError::RateLimit(body),
        401 | 403 => SendError::Auth(code, body),
        _ => SendError::Api(code, body),
    })
}
