# Gmail Draft Scheduler

Stand-alone binary that **sends Gmail drafts** the TUI bot has queued.
The bot creates drafts (Gmail API: `users.drafts.create`); the scheduler
sends them later (Gmail API: `users.drafts.send`) at the times you set in
`config.toml [schedule]`. The two processes are decoupled — the bot can be
closed; the scheduler keeps running under your OS service manager.

## Architecture in one minute

```
TUI bot  ──[Ctrl+E]──►  drafts.json  ──┐
                                       │
                                       ├──►  scheduler binary  ──►  Gmail API
token_cache.json  ◄────────────────────┘                            (drafts.send)
```

* `drafts.json` — queue file. One row per Gmail draft: `draft_id`, `to`,
  `company`, `send_at`, `status`, `attempts`.
* `token_cache.json` — OAuth access + refresh token. Shared by both
  processes; refresh happens silently when the access token expires.
* `sent_log.json` / `failed_log.json` — append-only outcome logs the
  scheduler writes.

All four files are git-ignored.

## One-time setup

### 1. Google Cloud Console

1. Create a project (any name).
2. **APIs & Services → Library** → enable **Gmail API**.
3. **APIs & Services → OAuth consent screen** →
   * User type: **External**
   * Add your own Gmail address as a test user
   * Scopes: add `https://www.googleapis.com/auth/gmail.compose`
4. **APIs & Services → Credentials** → **Create credentials → OAuth client ID**
   * Application type: **Desktop app**
   * Download the JSON.

### 2. Local credentials

```bash
cp .env.example .env
# Edit .env, paste client_id and client_secret from the downloaded JSON.

cp config.example.toml config.toml
# Edit [schedule], [gmail], [smtp] (smtp.from_address is the "From:" header
# Gmail uses when sending the draft).
```

### 3. Authenticate

```bash
cargo run --release --bin scheduler -- --setup
```

Opens your browser. Grant the Gmail-compose scope to the desktop client.
After consent, a tiny local HTTP server on `127.0.0.1` catches the redirect
and writes `token_cache.json`. Refresh-token rotation is automatic from
this point on.

## Day-to-day use

### Queue drafts from the TUI

```
cargo run --release
```

1. Paste URLs → `Ctrl+S` to scrape.
2. Review picked addresses.
3. Open compose (`Enter` or `Ctrl+S` on Review).
4. **`Ctrl+E`** — create Gmail drafts. Each draft gets a `send_at`
   spaced `interval_min` apart starting at the next `window_start`.
5. `Ctrl+G` (from URLs screen) — read-only Queue inspector.

### Run the scheduler

```bash
cargo run --release --bin scheduler
```

* Polls `drafts.json` every 15 minutes.
* Sends every pending draft whose `send_at ≤ now`, respecting:
  * `[schedule].window_start` / `window_end` (won't send outside)
  * `[schedule].days` (won't send on disabled days)
  * `[schedule].daily_limit` (stops once met)
  * `[schedule].interval_min` (sleeps between two sends in the same pass)
* Retry / error policy:
  * HTTP 429 → exponential backoff 5 → 15 → 60 min, then stop the pass
  * HTTP 401/403 → mark `Failed`, log auth error (you need to re-run
    `--setup`)
  * Network / other → up to 3 attempts, 30 min apart, then `Failed`

## Run as a service

### Linux (systemd)

```ini
# /etc/systemd/system/mail-scheduler.service
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

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now mail-scheduler
journalctl -u mail-scheduler -f      # tail logs
```

### macOS (launchd)

```xml
<!-- ~/Library/LaunchAgents/com.hired.scheduler.plist -->
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
 "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>            <string>com.hired.scheduler</string>
  <key>ProgramArguments</key>
  <array>
    <string>/Users/YOU/Projects/Hired/target/release/scheduler</string>
  </array>
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

1. Build a release binary: `cargo build --release --bin scheduler`.
2. Open **Task Scheduler → Create Task**.
3. **General**: name `mail-scheduler`; check *Run only when user is logged on*.
4. **Triggers**: *At log on*.
5. **Actions**: *Start a program*
   * Program: `C:\path\to\Hired\target\release\scheduler.exe`
   * Start in: `C:\path\to\Hired`
6. **Conditions / Settings**: uncheck *Stop the task if it runs longer than…*

## Tuning the schedule

Edit `config.toml [schedule]` directly. The schedule form in the TUI is
**deferred** for v0.1 — the in-app spec called for a dedicated form, but
editing the TOML file directly + reloading the scheduler is faster and
less code. Re-running the scheduler picks up the new values on the next
pass; no need to re-create drafts.

## Troubleshooting

| symptom | likely cause | fix |
|---|---|---|
| `no token cache at token_cache.json — run scheduler --setup first` | OAuth never completed | `cargo run --bin scheduler -- --setup` |
| `auth error 401/403` in `failed_log.json` | refresh token revoked (e.g. password changed) | re-run `--setup` |
| `429 backoff …` in logs | Gmail quota hit | wait, scheduler retries automatically |
| TUI says "Gmail credentials missing" | `.env` empty or not loaded | check `.env` next to `config.toml` |
| drafts created but never sent | scheduler not running | start it (or check `systemctl status mail-scheduler`) |
