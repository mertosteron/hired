use crate::config::SmtpConfig;
use crate::error::BotError;
use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use std::path::Path;

fn build_from(smtp: &SmtpConfig) -> Result<Mailbox, BotError> {
    let raw = if smtp.from_name.trim().is_empty() {
        smtp.from_address.clone()
    } else {
        format!("{} <{}>", smtp.from_name.trim(), smtp.from_address.trim())
    };
    Ok(raw.parse::<Mailbox>()?)
}

fn build_transport(
    smtp: &SmtpConfig,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, BotError> {
    let creds = Credentials::new(smtp.username.clone(), smtp.password.clone());
    let builder = if smtp.port == 465 {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp.server)?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp.server)?
    };
    Ok(builder.port(smtp.port).credentials(creds).build())
}

async fn attach_pdf(multipart: MultiPart, path: &str, fallback_name: &str) -> (MultiPart, bool) {
    let p = Path::new(path);
    if !p.exists() || !p.is_file() {
        return (multipart, false);
    }
    let Ok(bytes) = tokio::fs::read(p).await else {
        return (multipart, false);
    };
    let filename = p
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(fallback_name)
        .to_string();
    let ctype = ContentType::parse("application/pdf").unwrap_or(ContentType::TEXT_PLAIN);
    (multipart.singlepart(Attachment::new(filename).body(bytes, ctype)), true)
}

/// Send an email with CV attached and, optionally, a second transcript attachment.
/// Pass an empty string for `transcript_path` to skip the second attachment.
pub async fn send_email(
    smtp: &SmtpConfig,
    to: &str,
    subject: &str,
    body: &str,
    cv_path: &str,
    transcript_path: &str,
) -> Result<(), BotError> {
    let from = build_from(smtp)?;
    let to_mbox: Mailbox = to.trim().parse()?;

    let text_part = SinglePart::builder()
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string());

    let multipart = MultiPart::mixed().singlepart(text_part);

    // CV — required
    let path = Path::new(cv_path);
    if !path.exists() || !path.is_file() {
        return Err(BotError::MissingCv(cv_path.to_string()));
    }
    let bytes = tokio::fs::read(path).await?;
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("cv.pdf")
        .to_string();
    let ctype = ContentType::parse("application/pdf").unwrap_or(ContentType::TEXT_PLAIN);
    let multipart = multipart.singlepart(Attachment::new(filename).body(bytes, ctype));

    // Transcript — optional
    let (multipart, _) = if !transcript_path.is_empty() {
        attach_pdf(multipart, transcript_path, "transcript.pdf").await
    } else {
        (multipart, false)
    };

    let email = Message::builder()
        .from(from)
        .to(to_mbox)
        .subject(subject)
        .multipart(multipart)?;

    let mailer = build_transport(smtp)?;
    mailer.send(email).await?;
    Ok(())
}
