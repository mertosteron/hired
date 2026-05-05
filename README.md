# JobFinder

A terminal-based Rust bot that:

1. Takes a list of company website URLs.
2. Scrapes each site (homepage + likely "contact"/"careers" pages) for
   `mailto:` links and email addresses.
3. Lets you confirm which address to use per site in a TUI (built with
   `ratatui` + `crossterm`).
4. Sends a pre-written email with your CV attached as a PDF over SMTP
   (via `lettre`).

## Project layout

```
JobFinder/
├── Cargo.toml
├── config.example.toml
├── README.md
└── src/
    ├── main.rs       # entry point, terminal setup, tokio runtime
    ├── app.rs        # app state machine + event loop
    ├── ui.rs         # ratatui rendering for every screen
    ├── scraper.rs    # reqwest + scraper + regex email extraction
    ├── mailer.rs     # lettre multipart SMTP send with PDF attachment
    ├── config.rs     # TOML config (SMTP creds, defaults)
    └── error.rs      # unified error type
```

## Build

Requires a recent stable Rust toolchain (1.75+).

```bash
cd JobFinder
cargo build --release
```

The binary lands at `./target/release/jobfinder`.

## Configure

```bash
cp config.example.toml config.toml
$EDITOR config.toml         # set SMTP creds, sender name, default subject/body, cv_path
```

For Gmail, generate an [App Password](https://support.google.com/accounts/answer/185833)
and use it as `password`. Port `587` uses STARTTLS, port `465` uses implicit TLS —
both are supported automatically based on the port number.

Drop your CV next to the binary as `cv.pdf` (or point `cv_path` at any
PDF file).

## Run

```bash
./target/release/jobfinder
```

### Workflow inside the TUI

| Screen        | Keys |
| ------------- | ---- |
| **URLs**      | Type/paste one URL per line. `F2` or `Ctrl+S` starts scraping. `Ctrl+L` loads `urls.txt` from CWD. `Ctrl+Q` quits. |
| **Scraping**  | Auto-progresses. Wait for it to finish. |
| **Review**    | `↑/↓` pick site, `←/→` cycle email candidates, `Space` skip/include, `F2` continue. `Esc` returns to URLs. |
| **Compose**   | `Tab` cycles between Subject / Body / CV path. Type to edit. `F2` starts sending. `Esc` returns to review. |
| **Sending**   | Auto-progresses. |
| **Done**      | Per-site results. `q` or `Esc` to exit. |

## Notes

- The scraper crawls the homepage plus up to 5 anchor links whose href or
  text contains words like `contact`, `careers`, `jobs`, `about`, `hr`.
- Found addresses are filtered to drop obvious junk
  (e.g. `*.png`, `*@example.com`, `*@sentry.io`, Wix telemetry).
- SMTP failures, HTTP errors, and pages with no emails are reported per-row
  rather than aborting the whole run.
- Rate limit yourself responsibly. The bot pauses briefly between sends.
