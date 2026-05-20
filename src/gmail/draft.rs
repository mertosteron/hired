//! Gmail draft creation. Reuses `lettre` to build a full multipart RFC 822
//! message (with CV attachment), then base64url-encodes the bytes and POSTs
//! to `users.drafts.create`.
//!
//! The returned draft ID is what the scheduler later passes to `drafts.send`.

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use lettre::Message;
use serde::{Deserialize, Serialize};
use std::path::Path;

const DRAFTS_CREATE_URL: &str = "https://gmail.googleapis.com/gmail/v1/users/me/drafts";

/// Caller-supplied fields for one draft.
pub struct DraftRequest<'a> {
    pub from_address: &'a str,
    pub from_name: &'a str,
    pub to: &'a str,
    pub subject: &'a str,
    pub body: &'a str,
    pub cv_path: &'a str,
    /// Empty string = no second attachment.
    pub transcript_path: &'a str,
}

#[derive(Debug, Deserialize)]
struct DraftCreateResponse {
    id: String,
}

#[derive(Debug, Serialize)]
struct DraftCreatePayload {
    message: MessageRaw,
}
#[derive(Debug, Serialize)]
struct MessageRaw { raw: String }

/// Build the multipart MIME blob.
async fn build_raw_mime(req: &DraftRequest<'_>) -> Result<Vec<u8>> {
    let from_mbox: Mailbox = if req.from_name.trim().is_empty() {
        req.from_address.parse()?
    } else {
        format!("{} <{}>", req.from_name.trim(), req.from_address.trim()).parse()?
    };
    let to_mbox: Mailbox = req.to.trim().parse()?;

    let text_part = SinglePart::builder()
        .header(ContentType::TEXT_PLAIN)
        .body(req.body.to_string());

    let multipart = MultiPart::mixed().singlepart(text_part);

    // CV — required.
    let cv = Path::new(req.cv_path);
    if !cv.exists() || !cv.is_file() {
        return Err(anyhow!("CV file not found: {}", req.cv_path));
    }
    let cv_bytes = tokio::fs::read(cv).await
        .with_context(|| format!("reading CV {}", req.cv_path))?;
    let cv_name = cv.file_name().and_then(|s| s.to_str()).unwrap_or("cv.pdf").to_string();
    let cv_ctype = ContentType::parse("application/pdf").unwrap_or(ContentType::TEXT_PLAIN);
    let multipart = multipart.singlepart(Attachment::new(cv_name).body(cv_bytes, cv_ctype));

    // Transcript — optional.
    let multipart = if req.transcript_path.is_empty() {
        multipart
    } else {
        let tp = Path::new(req.transcript_path);
        if tp.exists() && tp.is_file() {
            let bytes = tokio::fs::read(tp).await.unwrap_or_default();
            let name = tp.file_name().and_then(|s| s.to_str()).unwrap_or("transcript.pdf").to_string();
            let ctype = ContentType::parse("application/pdf").unwrap_or(ContentType::TEXT_PLAIN);
            multipart.singlepart(Attachment::new(name).body(bytes, ctype))
        } else {
            multipart
        }
    };

    let message = Message::builder()
        .from(from_mbox)
        .to(to_mbox)
        .subject(req.subject)
        .multipart(multipart)?;

    Ok(message.formatted())
}

/// Create a Gmail draft and return the assigned draft ID.
pub async fn create_draft(
    access_token: &str,
    req: &DraftRequest<'_>,
) -> Result<String> {
    let raw = build_raw_mime(req).await?;
    let raw_b64 = URL_SAFE_NO_PAD.encode(&raw);

    let payload = DraftCreatePayload {
        message: MessageRaw { raw: raw_b64 },
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let resp = client
        .post(DRAFTS_CREATE_URL)
        .bearer_auth(access_token)
        .json(&payload)
        .send()
        .await
        .context("POST drafts.create")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("drafts.create {status}: {body}"));
    }
    let parsed: DraftCreateResponse = resp.json().await
        .context("parsing drafts.create response")?;
    log::info!("created Gmail draft {} for {}", parsed.id, req.to);
    Ok(parsed.id)
}
