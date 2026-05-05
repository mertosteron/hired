use thiserror::Error;

#[derive(Debug, Error)]
pub enum BotError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("no emails found on {0}")]
    NoEmails(String),

    #[error("missing CV file: {0}")]
    MissingCv(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("address error: {0}")]
    Address(#[from] lettre::address::AddressError),

    #[error("email build error: {0}")]
    EmailBuild(#[from] lettre::error::Error),

    #[error("SMTP error: {0}")]
    Smtp(#[from] lettre::transport::smtp::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
