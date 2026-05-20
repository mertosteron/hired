use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    pub server: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from_address: String,
    #[serde(default)]
    pub from_name: String,
}

impl Default for SmtpConfig {
    fn default() -> Self {
        Self {
            server: "smtp.gmail.com".into(),
            port: 587,
            username: String::new(),
            password: String::new(),
            from_address: String::new(),
            from_name: String::new(),
        }
    }
}

/// Scheduling rules for the Gmail-draft sender. Mirrors the `[schedule]`
/// TOML block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    /// Start of the daily send window, "HH:MM" 24-hour.
    #[serde(default = "default_window_start_str")]
    pub window_start: String,
    /// End of the daily send window, "HH:MM" 24-hour. Exclusive.
    #[serde(default = "default_window_end_str")]
    pub window_end: String,
    /// Three-letter weekday names (Mon/Tue/.../Sun) when sends are allowed.
    #[serde(default = "default_days")]
    pub days: Vec<String>,
    /// Minimum minutes between successive sends.
    #[serde(default = "default_interval_min")]
    pub interval_min: u32,
    /// Hard upper bound on sends per calendar day.
    #[serde(default = "default_schedule_daily_limit")]
    pub daily_limit: u32,
    /// IANA timezone for `window_start`/`window_end`/`days`. "Local" = system clock.
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            window_start: default_window_start_str(),
            window_end: default_window_end_str(),
            days: default_days(),
            interval_min: default_interval_min(),
            daily_limit: default_schedule_daily_limit(),
            timezone: default_timezone(),
        }
    }
}

/// Gmail OAuth + queue-file locations. Mirrors the `[gmail]` TOML block.
/// `client_id` / `client_secret` are read from `.env` (`GMAIL_CLIENT_ID`,
/// `GMAIL_CLIENT_SECRET`) and overridden onto the config at load time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmailConfig {
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    #[serde(default = "default_token_cache")]
    pub token_cache: String,
    #[serde(default = "default_drafts_db")]
    pub drafts_db: String,
    #[serde(default = "default_sent_log")]
    pub sent_log: String,
    #[serde(default = "default_failed_log")]
    pub failed_log: String,
}

impl Default for GmailConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret: String::new(),
            token_cache: default_token_cache(),
            drafts_db: default_drafts_db(),
            sent_log: default_sent_log(),
            failed_log: default_failed_log(),
        }
    }
}

fn default_window_start_str() -> String { "09:30".to_string() }
fn default_window_end_str() -> String { "17:00".to_string() }
fn default_days() -> Vec<String> {
    vec!["Mon", "Tue", "Wed", "Thu", "Fri"].into_iter().map(String::from).collect()
}
fn default_interval_min() -> u32 { 45 }
fn default_schedule_daily_limit() -> u32 { 15 }
fn default_token_cache() -> String { "token_cache.json".to_string() }
fn default_drafts_db() -> String { "drafts.json".to_string() }
fn default_sent_log() -> String { "sent_log.json".to_string() }
fn default_failed_log() -> String { "failed_log.json".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub smtp: SmtpConfig,
    #[serde(default)]
    pub schedule: ScheduleConfig,
    #[serde(default)]
    pub gmail: GmailConfig,
    #[serde(default = "default_subject")]
    pub default_subject: String,
    #[serde(default = "default_body")]
    pub default_body: String,
    #[serde(default = "default_cv")]
    pub cv_path: String,
    /// Optional second attachment (e.g. transcript). Leave empty to skip.
    #[serde(default)]
    pub transcript_path: String,
    /// Minimum seconds to wait between emails (spam protection).
    #[serde(default = "default_delay_min")]
    pub send_delay_min_secs: u64,
    /// Maximum seconds to wait between emails (actual delay is random in [min, max]).
    #[serde(default = "default_delay_max")]
    pub send_delay_max_secs: u64,
    /// Maximum emails sent per session (daily guard).
    #[serde(default = "default_daily_limit")]
    pub daily_limit: usize,
    /// Hour (0-23) before which no emails are sent.
    #[serde(default = "default_window_start")]
    pub send_window_start: u32,
    /// Hour (0-23) at or after which no emails are sent.
    #[serde(default = "default_window_end")]
    pub send_window_end: u32,
    /// IANA timezone name for the send window (e.g. "Europe/Istanbul"). "Local" uses system time.
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

fn default_subject() -> String {
    "Application for Open Position".to_string()
}

fn default_body() -> String {
    "Hello,\n\nPlease find my CV attached.\n\nBest regards.".to_string()
}

fn default_cv() -> String {
    "cv.pdf".to_string()
}

fn default_timezone() -> String { "Local".to_string() }
fn default_delay_min() -> u64 { 60 }
fn default_delay_max() -> u64 { 240 }
fn default_daily_limit() -> usize { 50 }
fn default_window_start() -> u32 { 8 }
fn default_window_end() -> u32 { 22 }

impl Config {
    /// Apply `.env` overrides — Gmail OAuth credentials must NEVER live in
    /// `config.toml`. Reads `GMAIL_CLIENT_ID` / `GMAIL_CLIENT_SECRET` and
    /// writes them onto `self.gmail`.
    pub fn apply_env_overrides(&mut self) {
        // dotenvy::dotenv() is best-effort; ignore missing .env file.
        let _ = dotenvy::dotenv();
        if let Ok(v) = std::env::var("GMAIL_CLIENT_ID") {
            if !v.is_empty() { self.gmail.client_id = v; }
        }
        if let Ok(v) = std::env::var("GMAIL_CLIENT_SECRET") {
            if !v.is_empty() { self.gmail.client_secret = v; }
        }
    }

    pub fn load_or_default<P: AsRef<Path>>(path: P) -> (Self, Option<String>) {
        match std::fs::read_to_string(&path) {
            Ok(s) => match toml::from_str::<Config>(&s) {
                Ok(c) => (c, None),
                Err(e) => (
                    Config {
                        default_subject: default_subject(),
                        default_body: default_body(),
                        cv_path: default_cv(),
                        send_delay_min_secs: default_delay_min(),
                        send_delay_max_secs: default_delay_max(),
                        daily_limit: default_daily_limit(),
                        send_window_start: default_window_start(),
                        send_window_end: default_window_end(),
                        ..Default::default()
                    },
                    Some(format!("config.toml parse error: {e}")),
                ),
            },
            Err(_) => (
                Config {
                    default_subject: default_subject(),
                    default_body: default_body(),
                    cv_path: default_cv(),
                    send_delay_min_secs: default_delay_min(),
                    send_delay_max_secs: default_delay_max(),
                    daily_limit: default_daily_limit(),
                    send_window_start: default_window_start(),
                    send_window_end: default_window_end(),
                    timezone: default_timezone(),
                    ..Default::default()
                },
                Some("config.toml not found — copy config.example.toml".into()),
            ),
        }
    }
}
