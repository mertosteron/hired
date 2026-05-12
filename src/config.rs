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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub smtp: SmtpConfig,
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

fn default_delay_min() -> u64 { 60 }
fn default_delay_max() -> u64 { 240 }
fn default_daily_limit() -> usize { 50 }
fn default_window_start() -> u32 { 8 }
fn default_window_end() -> u32 { 22 }

impl Config {
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
                    ..Default::default()
                },
                Some("config.toml not found — copy config.example.toml".into()),
            ),
        }
    }
}
