# Hired

A terminal bot that scrapes company websites for HR / careers email addresses,
turns each one into a personalised Gmail **draft**, and lets a background
scheduler send those drafts over the next days at human-looking intervals.

```
Paste URLs → Scrape emails → Pick targets → Write message → Queue drafts → Scheduler sends them
```

The TUI and the scheduler are decoupled. You can close the TUI right after
queuing — the scheduler keeps sending under system d/ launch d/ Task Scheduler.

---

## What you get out of the box

* **5-stage email scraper** per site (HTML text, `mailto:` links, sub-pages,
  element attributes, inline `<script>` blocks). HR-looking addresses
  (`hr@`, `careers@`, `jobs@`, …) float to the top; `noreply@`, tracking
  domains, and image filenames are filtered out.
* **TUI review screen** so you can pick the right address per site instead
  of blasting blindly.
* **Gmail draft creation** via the official Gmail API — the drafts show up
  inside your real Gmail "Drafts" folder, with your CV attached.
* **Scheduler binary** that sends those drafts later, respecting a daily
  send window, weekday list, per-interval pacing, and a daily cap.
* **CSV / JSON logs** for everything that goes out.

---

## Step-by-step: first-time setup

### 1. Install Rust

Get the toolchain from [rustup.rs](https://rustup.rs) (Rust 1.75 or newer).

### 2. Clone and build

```bash
git clone https://github.com/mertosteron/hired.git
cd Hired
cargo build --release
```

This produces two binaries you'll use:

* `target/release/hired`     — the TUI bot
* `target/release/scheduler` — the background sender

(You can also `cargo install --path .` to put `hired` on your `PATH`.)

### 3. Create a Google Cloud project (one time)

The scheduler talks to Gmail through the official API, so it needs an OAuth
client.

1. Go to [console.cloud.google.com](https://console.cloud.google.com), create
   a project (any name).
2. **APIs & Services → Library** → search for **Gmail API** → **Enable**.
3. **APIs & Services → OAuth consent screen**
   * **Get started**
   * App name: **Hired**, User support email: **your own Gmail address**
   * User type: **External**
   * Add **your own Gmail address** as a *test user*
   * Scopes: add `https://www.googleapis.com/auth/gmail.compose`
4. **APIs & Services → Credentials → Create credentials → OAuth client ID**
   * Application type: **Desktop app**
   * Download the JSON file Google gives you — you'll copy two values out of
     it next.

### 4. Fill in `.env`

```bash
cp .env.example .env
```

Open `.env` and paste the two values from the JSON you just downloaded:

```env
GMAIL_CLIENT_ID=xxxxxxxxxxxx.apps.googleusercontent.com
GMAIL_CLIENT_SECRET=xxxxxxxxxxxxxxx
```

`.env` is git-ignored.

### 5. Fill in `config.toml`

```bash
cp config.example.toml config.toml
```

Open `config.toml` and edit:

* `default_subject` / `default_body` — your default outreach message
* `cv_path` — relative path to your CV PDF (e.g. `cv.pdf` next to the binary)
* `transcript_path` — second attachment, or leave `""`
* `[schedule]` — when the scheduler is allowed to send:
  * `window_start` / `window_end` — daily send window, `HH:MM`
  * `days` — weekdays it's allowed to fire (e.g. `["Mon","Tue","Wed","Thu","Fri"]`)
  * `interval_min` — minutes between two consecutive sends
  * `daily_limit` — hard cap per calendar day
  * `timezone` — e.g. `"Europe/Istanbul"` or `"Local"`
* `[smtp].from_address` / `from_name` — the "From:" header Gmail will use
  when it sends the draft (the rest of `[smtp]` is unused once you're on
  Gmail-API mode but kept for compatibility)

Keep `config.toml`, `.env`, and your CV all in the directory you run the
binary from.

### 6. Authenticate once with Google

```bash
cargo run --release --bin scheduler -- --setup
```

This opens your browser, asks you to grant the Gmail-compose scope, and
writes `token_cache.json` next to your config. Refreshes happen silently
from then on — you only re-run `--setup` if Google revokes the token (e.g.
password change).

You're done with setup.

---

## Daily flow: queue some drafts

```bash
cargo run --release             # or just `hired` if installed
```

You'll move through six screens. Keys are also shown in the status bar of
each screen.

### Screen 1 — URLs

| Key            | What it does                                    |
|----------------|--------------------------------------------------|
| Type / paste   | One company URL per line                        |
| `Ctrl+L`       | Load URLs from `urls.txt` in the working dir    |
| `Ctrl+S`       | Start scraping                                  |
| `Ctrl+G`       | Open the read-only Queue inspector (see below)  |
| `Ctrl+H`       | Help overlay                                    |
| `Esc`          | Quit                                            |

### Screen 2 — Scraping

Runs automatically. You'll see per-site progress: pages fetched, candidates
found. Wait for it to finish.

### Screen 3 — Review

One row per site. The bot has already picked the most likely HR address; you
can cycle through alternatives or skip a site entirely.

| Key                | What it does                                |
|--------------------|---------------------------------------------|
| `↑` / `↓` (`j/k`) | Move between sites                          |
| `←` / `→`         | Cycle through the candidate emails for the selected site |
| `Space`            | Include / skip this site                    |
| `Enter` or `Ctrl+S`| Continue to Compose                         |
| `Esc`              | Back to URLs                                |

### Screen 4 — Compose

Edit the subject, body, CV path, and transcript path. The defaults come from
`config.toml`. Body supports multi-line editing.

| Key            | What it does                                          |
|----------------|--------------------------------------------------------|
| `Tab` / `BackTab` | Cycle fields (subject → body → CV path → transcript) |
| `Ctrl+S`       | **Create Gmail drafts** for every included site        |
| `Esc`          | Back to Review                                         |

When you press `Ctrl+S`:

* Each picked site becomes a Gmail draft via `users.drafts.create`.
* Drafts get a `send_at` spaced `interval_min` minutes apart, starting at
  the next `window_start` slot allowed by your schedule.
* The queue is persisted to `drafts.json` (git-ignored). The TUI does **not**
  send them — that's the scheduler's job.

### Screen 5 — Enqueue progress

Shows one line per draft as it's created in Gmail. Failures (network, auth)
are reported here.

### Screen 6 — Done

Summary of how many drafts were queued, and reminders for what to do next
(start the scheduler, or close the TUI safely).

### Queue inspector — `Ctrl+G` from URLs screen

Read-only view of `drafts.json`: every draft, its `send_at`, status,
attempts. `R` reloads from disk; `Esc` returns. Useful for sanity-checking
what's scheduled without opening the JSON file by hand.

---

## Daily flow: actually send the drafts

```bash
cargo run --release --bin scheduler        # foreground
```

What the scheduler does on every pass (every 15 minutes):

1. Loads `drafts.json`.
2. Picks every `Pending` draft whose `send_at ≤ now`.
3. Refuses to send if we're outside `window_start`/`window_end`, the weekday
   is disabled, or we've already hit `daily_limit` today.
4. For each draft: `users.drafts.send`, then mark as `Sent` in
   `drafts.json` and append to `sent_log.json`.
5. Sleeps `interval_min` between two sends in the same pass.

Retry / error policy:

* HTTP 429 (Gmail quota) → exponential backoff 5 → 15 → 60 min, stop this
  pass and try later.
* HTTP 401 / 403 (auth) → mark `Failed`, log to `failed_log.json`. Re-run
  `scheduler --setup`.
* Network / 5xx → up to 3 attempts, 30 min apart, then `Failed`.

You can stop the scheduler with `Ctrl+C`. The next start picks up where it
left off — no draft is lost.

---

## Run the scheduler as a background service

You almost certainly want this — outreach takes days, not one terminal
session.

### Linux (systemd)

Create `/etc/systemd/system/mail-scheduler.service`:

```ini
[Unit]
Description=Hired :: Gmail Draft Scheduler
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=YOUR_USER
WorkingDirectory=/home/YOUR_USER/Projects/Hired
ExecStart=/home/YOUR_USER/Projects/Hired/target/release/scheduler
Restart=always
RestartSec=10
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
```

Then:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now mail-scheduler
journalctl -u mail-scheduler -f          # tail logs live
```

### macOS (launchd)

`~/Library/LaunchAgents/com.hired.scheduler.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
 "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>            <string>com.hired.scheduler</string>
  <key>ProgramArguments</key>
  <array><string>/Users/YOU/Projects/Hired/target/release/scheduler</string></array>
  <key>WorkingDirectory</key> <string>/Users/YOU/Projects/Hired</string>
  <key>RunAtLoad</key>        <true/>
  <key>KeepAlive</key>        <true/>
  <key>StandardOutPath</key>  <string>/tmp/mail-scheduler.out</string>
  <key>StandardErrorPath</key><string>/tmp/mail-scheduler.err</string>
</dict>
</plist>
```

```bash
launchctl load -w ~/Library/LaunchAgents/com.hired.scheduler.plist
launchctl list | grep hired
```

### Windows (Task Scheduler)

1. `cargo build --release --bin scheduler`
2. **Task Scheduler → Create Task**
3. **General**: name `mail-scheduler`, check *Run only when user is logged on*
4. **Triggers**: *At log on*
5. **Actions** → *Start a program*
   * Program: `C:\path\to\Hired\target\release\scheduler.exe`
   * Start in: `C:\path\to\Hired`
6. **Conditions / Settings**: uncheck *Stop the task if it runs longer than…*

---

## Files the app touches

| File                | Created by   | What's in it                                  | Git-ignored |
|---------------------|--------------|------------------------------------------------|-------------|
| `config.toml`       | you          | TUI + scheduler settings                       | yes         |
| `.env`              | you          | `GMAIL_CLIENT_ID`, `GMAIL_CLIENT_SECRET`       | yes         |
| `urls.txt`          | you (opt.)   | One URL per line, loaded by `Ctrl+L`           | no          |
| `cv.pdf`            | you          | Your CV (path from `cv_path`)                  | yes         |
| `token_cache.json`  | scheduler    | OAuth access + refresh token                   | yes         |
| `drafts.json`       | TUI          | The queue. One row per Gmail draft.            | yes         |
| `sent_log.json`     | scheduler    | Append-only log of successful sends            | yes         |
| `failed_log.json`   | scheduler    | Append-only log of permanent failures          | yes         |
| `send_log_*.csv`    | TUI (legacy SMTP mode) | Per-session CSV of sends             | yes         |
| `blocked_domains.toml` | repo      | Domains the scraper auto-discards              | no          |

---

## Tuning the schedule later

Edit `config.toml [schedule]` directly. The scheduler picks up the new
values on its next pass — no need to re-create drafts. There is no
schedule-editor screen inside the TUI (deferred for v0.1).

---

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `no token cache at token_cache.json — run scheduler --setup first` | OAuth never completed | `cargo run --bin scheduler -- --setup` |
| `auth error 401/403` in `failed_log.json` | refresh token revoked (e.g. password changed) | re-run `--setup` |
| `429 backoff …` in scheduler logs | Gmail per-user quota hit | wait, scheduler retries automatically |
| TUI says "Gmail credentials missing" | `.env` empty / not in working directory | check `.env` next to `config.toml` |
| Drafts created but never sent | scheduler not running | start it (or `systemctl status mail-scheduler`) |
| `hired` command not found | Cargo bin dir not on PATH | `export PATH="$HOME/.cargo/bin:$PATH"` |

---

## Spam / etiquette guardrails (defaults)

* `interval_min = 45` — at least 45 min between two sends
* `daily_limit = 15` — at most 15 sends per calendar day
* `window_start = 09:30`, `window_end = 17:00` — business hours only
* `days = Mon..Fri` — no weekend sends
* `from_name` / `from_address` — uses your real Gmail address; replies land
  in your inbox like any other email

You can dial these up, but keep in mind: Gmail will flag accounts that
behave like bots. The defaults are deliberately conservative.

---

## Project layout

```
src/
├── main.rs        — entry point for `hired` TUI: terminal setup, tokio runtime
├── lib.rs         — shared module declarations
├── app.rs         — state machine + event loop driving the TUI
├── ui.rs          — ratatui rendering for every screen
├── scraper.rs     — 5-stage email scraper
├── mailer.rs      — legacy SMTP sender (still compiled for fallback / testing)
├── config.rs      — TOML config loader + env overrides
├── queue.rs       — drafts.json read/write + scheduling math
├── history.rs     — sent_log / failed_log append helpers
├── error.rs       — unified error type
├── gmail/
│   ├── auth.rs    — OAuth device-loopback flow, token cache, refresh
│   ├── draft.rs   — users.drafts.create wrapper
│   ├── sender.rs  — users.drafts.send wrapper
│   └── mod.rs     — re-exports
└── bin/
    ├── scheduler.rs — long-running sender daemon (also handles `--setup`)
    └── verify.rs    — one-shot dev tool to verify SMTP creds / Gmail token
```
