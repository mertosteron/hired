use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentEntry {
    pub email: String,
    pub date: String,
    pub url: String,
    pub company_name: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SentHistory {
    #[serde(default)]
    pub entries: Vec<SentEntry>,
}

impl SentHistory {
    pub fn path() -> PathBuf {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                return parent.join("sent_history.json");
            }
        }
        PathBuf::from("sent_history.json")
    }

    pub fn load() -> Self {
        let p = Self::path();
        let Ok(s) = std::fs::read_to_string(&p) else { return Self::default() };
        serde_json::from_str(&s).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let s = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into());
        std::fs::write(Self::path(), s)
    }

    pub fn record(&mut self, entry: SentEntry) {
        self.entries.push(entry);
    }

    /// Set of all contacted emails (lowercased), useful for fast lookup.
    pub fn contacted_set(&self) -> HashSet<String> {
        self.entries
            .iter()
            .map(|e| e.email.trim().to_ascii_lowercase())
            .collect()
    }

    /// Entries grouped by date, newest first.
    pub fn by_date_desc(&self) -> Vec<(String, Vec<&SentEntry>)> {
        let mut dates: Vec<&String> = self.entries.iter().map(|e| &e.date).collect();
        dates.sort();
        dates.dedup();
        dates.reverse();
        dates
            .into_iter()
            .map(|d| {
                let group: Vec<&SentEntry> =
                    self.entries.iter().filter(|e| &e.date == d).collect();
                (d.clone(), group)
            })
            .collect()
    }
}
