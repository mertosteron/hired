# Hired

A terminal bot that finds HR email addresses on company websites and sends your CV automatically.

```
Paste URLs → Scrape emails → Pick targets → Write message → Send
```

---

## What it does

1. Scrapes each URL you provide (homepage + contact/careers sub-pages) for email addresses.
2. Shows you the candidates per site so you can pick the right one.
3. Sends your message with CV and (optionally) a transcript attached.
4. Waits a random 1–4 minutes between emails to avoid spam filters.
5. Saves every send result to `send_log_YYYYMMDD.csv`.

---

## Install

Requires Rust 1.75+. Get it from [rustup.rs](https://rustup.rs).

```bash
cargo install --path .
```

Then run `hired` from any terminal.

To update: `cargo install --path . --force`  
To uninstall: `cargo uninstall hired`

> If `hired` is not found, add Cargo's bin directory to your PATH:
> ```bash
> export PATH="$HOME/.cargo/bin:$PATH"
> ```

---

## Configuration

```bash
cp config.example.toml config.toml
```

Fill in `config.toml`:

```toml
default_subject = "Internship Application — Your Name"
default_body    = """Hello, ..."""
cv_path         = "cv.pdf"
transcript_path = "transcript.pdf"  # leave empty to skip

send_delay_min_secs = 60    # minimum wait between emails (seconds)
send_delay_max_secs = 240   # maximum wait between emails (seconds)
daily_limit         = 50    # max emails per session
send_window_start   = 8     # earliest hour to send (0–23)
send_window_end     = 22    # latest hour to send (0–23)

[smtp]
server       = "smtp.gmail.com"
port         = 587           # 587 = STARTTLS, 465 = implicit TLS
username     = "you@gmail.com"
password     = "app-password"
from_address = "you@gmail.com"
from_name    = "Your Name"
```

> **Gmail users:** your normal password won't work. Generate an
> [App Password](https://support.google.com/accounts/answer/185833) and use that instead.

Keep `config.toml` and your CV in the same directory and launch `hired` from there.

---

## Usage

```bash
hired
```

### Screens

| Screen | Keys |
|---|---|
| **URLs** | Type or paste one URL per line. `F2` starts scraping. `Ctrl+L` loads `urls.txt`. `Ctrl+Q` quits. |
| **Scraping** | Runs automatically — just wait. |
| **Review** | `↑/↓` select site · `←/→` cycle email candidates · `Space` include/skip · `F2` continue · `Esc` back. |
| **Compose** | `Tab/BackTab` cycles fields (subject / body / CV path / transcript path). `F2` starts sending. `Esc` back. |
| **Sending** | Runs automatically with a random delay between each email. |
| **Done** | Per-site results shown. `send_log_YYYYMMDD.csv` written. `q` or `Esc` to exit. |

---

## How it works

**5-stage email discovery per site:**

1. Full HTML text scan — regex over the raw page source
2. `mailto:` href attributes
3. Up to 8 contact/careers/about sub-pages followed and scanned
4. All HTML element attributes (catches `data-email` and similar hidden fields)
5. `<script>` tag bodies (catches emails embedded in JavaScript)

Found addresses are ranked by relevance — `hr@`, `careers@`, `jobs@` float to the top; `noreply@` and tracking domains are filtered out.

**Spam protection:**

- Random delay between emails (default: 1–4 minutes)
- Per-session send limit (default: 50)
- Time-window enforcement — no emails sent outside configured hours (Recommended hours : 08:00–10:00)

---

## Project layout

```
src/
├── main.rs     — entry point, terminal setup, tokio runtime
├── app.rs      — state machine and event loop
├── ui.rs       — ratatui rendering for every screen
├── scraper.rs  — 5-stage email scraper
├── mailer.rs   — SMTP sender with CV + transcript attachments
├── config.rs   — TOML config loader
└── error.rs    — unified error type
```
