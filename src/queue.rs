//! File-backed draft queue shared by the TUI bot and the scheduler binary.
//!
//! The bot creates Gmail drafts and writes one [`ScheduledDraft`] per row to
//! `drafts.json`. The scheduler binary polls that file, picks up rows whose
//! `send_at` has passed, fires the Gmail `drafts.send` API, and moves the
//! outcome to either `sent_log.json` or `failed_log.json`.
//!
//! All three files are atomically rewritten (write-temp-then-rename) so a
//! crash mid-write can't corrupt the queue.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Lifecycle of a scheduled draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DraftStatus {
    Pending,
    Sent,
    Failed,
}

/// One row in `drafts.json` — a Gmail draft that has been created but not yet
/// sent. `draft_id` is the ID returned by Gmail's `drafts.create` call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledDraft {
    pub draft_id: String,
    pub company: String,
    pub to: String,
    pub send_at: DateTime<Utc>,
    pub status: DraftStatus,
    #[serde(default)]
    pub attempts: u8,
    /// Free-form note — last error message, retry reason, etc.
    #[serde(default)]
    pub note: String,
}

/// On-disk envelope so we can version the queue file if the schema changes.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DraftQueue {
    #[serde(default)]
    pub drafts: Vec<ScheduledDraft>,
}

impl DraftQueue {
    /// Load from `path`. Missing file returns an empty queue; corrupt file is
    /// treated as empty so the scheduler can keep running.
    pub fn load<P: AsRef<Path>>(path: P) -> Self {
        let Ok(s) = std::fs::read_to_string(path.as_ref()) else { return Self::default() };
        serde_json::from_str(&s).unwrap_or_default()
    }

    /// Atomically replace the file at `path` with the current queue state.
    pub fn save<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let body = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into());
        atomic_write(path.as_ref(), body.as_bytes())
    }

    pub fn pending(&self) -> impl Iterator<Item = &ScheduledDraft> {
        self.drafts.iter().filter(|d| d.status == DraftStatus::Pending)
    }
}

/// Append-only log of every send outcome. Same schema for `sent_log.json` and
/// `failed_log.json`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct OutcomeLog {
    #[serde(default)]
    pub entries: Vec<OutcomeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutcomeEntry {
    pub draft_id: String,
    pub company: String,
    pub to: String,
    pub at: DateTime<Utc>,
    /// Empty for successful sends; populated for failures.
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub attempts: u8,
}

impl OutcomeLog {
    pub fn load<P: AsRef<Path>>(path: P) -> Self {
        let Ok(s) = std::fs::read_to_string(path.as_ref()) else { return Self::default() };
        serde_json::from_str(&s).unwrap_or_default()
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let body = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into());
        atomic_write(path.as_ref(), body.as_bytes())
    }

    pub fn push(&mut self, entry: OutcomeEntry) { self.entries.push(entry); }

    /// Count entries whose `at` falls on the same UTC date as `now`.
    pub fn count_for_day(&self, now: DateTime<Utc>) -> usize {
        let today = now.date_naive();
        self.entries.iter().filter(|e| e.at.date_naive() == today).count()
    }
}

fn atomic_write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp: PathBuf = match path.extension() {
        Some(ext) => {
            let mut p = path.to_path_buf();
            let mut new_ext = ext.to_os_string();
            new_ext.push(".tmp");
            p.set_extension(new_ext);
            p
        }
        None => {
            let mut p = path.to_path_buf();
            p.set_extension("tmp");
            p
        }
    };
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
