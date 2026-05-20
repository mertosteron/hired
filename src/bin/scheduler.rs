//! Standalone Gmail-draft scheduler.
//!
//! Two modes:
//! - `scheduler --setup` — runs the one-time OAuth consent flow and writes
//!   `token_cache.json`.
//! - `scheduler` (no flags) — polling loop that reads `drafts.json`,
//!   refreshes the OAuth token if needed, sends every pending draft whose
//!   `send_at` has passed, and records the outcome in `sent_log.json` /
//!   `failed_log.json`. Sleeps 15 minutes between polls. Survives the bot
//!   being closed.
//!
//! Designed to run under `systemd` / `launchd` / Windows Task Scheduler.

use chrono::{DateTime, Datelike, Duration as ChronoDuration, NaiveTime, TimeZone, Utc, Weekday};
use std::time::Duration;

use hired::config::{Config, ScheduleConfig};
use hired::gmail::{ensure_token, run_setup_flow, send_draft, SendError};
use hired::queue::{DraftQueue, DraftStatus, OutcomeEntry, OutcomeLog};

/// Hard ceiling on attempts. After this the draft is moved to failed_log.
const MAX_ATTEMPTS: u8 = 3;
/// Tail of the polling sleep cycle.
const POLL_INTERVAL: Duration = Duration::from_secs(60 * 15);

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let (mut config, warning) = Config::load_or_default("config.toml");
    config.apply_env_overrides();
    if let Some(w) = warning {
        log::warn!("{}", w);
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--setup") {
        log::info!("running OAuth setup flow");
        run_setup_flow(&config.gmail).await?;
        println!("Gmail auth complete. Token cached at {}", config.gmail.token_cache);
        return Ok(());
    }

    log::info!("scheduler starting — polling every {}s", POLL_INTERVAL.as_secs());
    loop {
        if let Err(e) = run_one_pass(&config).await {
            log::error!("scheduler pass failed: {e}");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// One iteration: load queue, decide what's eligible, send, persist.
async fn run_one_pass(config: &Config) -> anyhow::Result<()> {
    let now = Utc::now();
    if !is_within_window(&config.schedule, now) {
        log::info!(
            "outside send window ({} / {}) — skipping",
            config.schedule.window_start, config.schedule.window_end
        );
        return Ok(());
    }

    let sent_log_path = &config.gmail.sent_log;
    let failed_log_path = &config.gmail.failed_log;
    let drafts_path = &config.gmail.drafts_db;

    let mut queue = DraftQueue::load(drafts_path);
    let mut sent_log = OutcomeLog::load(sent_log_path);
    let mut failed_log = OutcomeLog::load(failed_log_path);

    let already_sent_today = sent_log.count_for_day(now);
    let limit = config.schedule.daily_limit as usize;
    if already_sent_today >= limit {
        log::info!("daily limit {limit} already met today");
        return Ok(());
    }
    let mut remaining_today = limit - already_sent_today;

    let token = ensure_token(&config.gmail).await?;

    let mut to_send: Vec<usize> = Vec::new();
    for (i, d) in queue.drafts.iter().enumerate() {
        if d.status != DraftStatus::Pending { continue; }
        if d.send_at > now { continue; }
        to_send.push(i);
    }

    for i in to_send {
        if remaining_today == 0 {
            log::info!("daily limit reached mid-pass");
            break;
        }
        let draft_id = queue.drafts[i].draft_id.clone();
        let res = send_draft(&token.access_token, &draft_id).await;
        let entry = &mut queue.drafts[i];
        entry.attempts = entry.attempts.saturating_add(1);
        match res {
            Ok(()) => {
                entry.status = DraftStatus::Sent;
                sent_log.push(OutcomeEntry {
                    draft_id: entry.draft_id.clone(),
                    company: entry.company.clone(),
                    to: entry.to.clone(),
                    at: Utc::now(),
                    error: String::new(),
                    attempts: entry.attempts,
                });
                remaining_today -= 1;
                // Respect inter-mail interval inside the same pass.
                let gap = Duration::from_secs(config.schedule.interval_min as u64 * 60);
                tokio::time::sleep(gap).await;
            }
            Err(SendError::RateLimit(msg)) => {
                let backoff_min = match entry.attempts {
                    1 => 5,
                    2 => 15,
                    _ => 60,
                };
                entry.send_at = Utc::now() + ChronoDuration::minutes(backoff_min);
                entry.note = format!("429 backoff {backoff_min}m: {msg}");
                log::warn!(
                    "draft {} rate-limited; retry in {}m",
                    entry.draft_id, backoff_min
                );
                break;
            }
            Err(SendError::Auth(code, msg)) => {
                entry.status = DraftStatus::Failed;
                entry.note = format!("auth error {code}: {msg}");
                log::error!(
                    "auth failure on {} — re-run --setup",
                    entry.draft_id
                );
                failed_log.push(OutcomeEntry {
                    draft_id: entry.draft_id.clone(),
                    company: entry.company.clone(),
                    to: entry.to.clone(),
                    at: Utc::now(),
                    error: entry.note.clone(),
                    attempts: entry.attempts,
                });
                break;
            }
            Err(other) => {
                if entry.attempts >= MAX_ATTEMPTS {
                    entry.status = DraftStatus::Failed;
                    entry.note = format!("{other}");
                    failed_log.push(OutcomeEntry {
                        draft_id: entry.draft_id.clone(),
                        company: entry.company.clone(),
                        to: entry.to.clone(),
                        at: Utc::now(),
                        error: entry.note.clone(),
                        attempts: entry.attempts,
                    });
                } else {
                    entry.send_at = Utc::now() + ChronoDuration::minutes(30);
                    entry.note = format!("retry: {other}");
                    log::warn!(
                        "draft {} failed, will retry: {other}",
                        entry.draft_id
                    );
                }
            }
        }
    }

    queue.save(drafts_path)?;
    sent_log.save(sent_log_path)?;
    failed_log.save(failed_log_path)?;
    Ok(())
}

/// True if `now` (in the configured timezone) falls inside the daily window
/// and the weekday is enabled.
pub fn is_within_window(cfg: &ScheduleConfig, now: DateTime<Utc>) -> bool {
    let local = to_zoned(cfg, now);
    let allowed = cfg.days.iter().any(|d| matches_weekday(local.weekday(), d));
    if !allowed { return false; }
    let Some(start) = parse_hhmm(&cfg.window_start) else { return true };
    let Some(end) = parse_hhmm(&cfg.window_end) else { return true };
    let t = local.time();
    t >= start && t < end
}

fn matches_weekday(wd: Weekday, label: &str) -> bool {
    let want = match label.to_ascii_lowercase().as_str() {
        "mon" | "monday" | "pzt" | "pazartesi" => Weekday::Mon,
        "tue" | "tuesday" | "sal" | "salı" | "sali" => Weekday::Tue,
        "wed" | "wednesday" | "çar" | "car" | "çarşamba" | "carsamba" => Weekday::Wed,
        "thu" | "thursday" | "per" | "perşembe" | "persembe" => Weekday::Thu,
        "fri" | "friday" | "cum" | "cuma" => Weekday::Fri,
        "sat" | "saturday" | "cmt" | "cumartesi" => Weekday::Sat,
        "sun" | "sunday" | "paz" | "pazar" => Weekday::Sun,
        _ => return false,
    };
    wd == want
}

fn parse_hhmm(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s.trim(), "%H:%M").ok()
}

fn to_zoned(cfg: &ScheduleConfig, now: DateTime<Utc>) -> chrono::NaiveDateTime {
    if cfg.timezone.is_empty() || cfg.timezone.eq_ignore_ascii_case("local") {
        return now.with_timezone(&chrono::Local).naive_local();
    }
    match cfg.timezone.parse::<chrono_tz::Tz>() {
        Ok(tz) => tz.from_utc_datetime(&now.naive_utc()).naive_local(),
        Err(_) => now.with_timezone(&chrono::Local).naive_local(),
    }
}
