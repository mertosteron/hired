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
                    ..Default::default()
                },
                Some("config.toml not found — copy config.example.toml".into()),
            ),
        }
    }
}
