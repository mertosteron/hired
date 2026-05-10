# Hired

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
Hired/
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

## Install

Requires a recent stable Rust toolchain (1.75+). Install via
[rustup](https://rustup.rs) — this also adds `~/.cargo/bin` (or
`%USERPROFILE%\.cargo\bin` on Windows) to your `PATH`.

From the project root:

```bash
cargo install --path .
```

This builds in release mode and drops a `hired` executable into Cargo's
bin directory. After that, you can launch the app from any terminal —
on Linux, macOS, or Windows — by typing:

```bash
hired
```

To update after pulling new changes, run `cargo install --path . --force`.

To uninstall: `cargo uninstall hired`.

### PATH troubleshooting

If `hired` is not found after installation, make sure Cargo's bin
directory is on your `PATH`:

- **Linux / macOS** — add to `~/.bashrc`, `~/.zshrc`, or equivalent:
  ```bash
  export PATH="$HOME/.cargo/bin:$PATH"
  ```
- **Windows (PowerShell)** — verify `%USERPROFILE%\.cargo\bin` is in
  your user `PATH` (rustup adds it automatically; restart the terminal
  if you just installed Rust).

### Build without installing

If you only want a local binary:

```bash
cargo build --release
./target/release/hired        # Linux / macOS
.\target\release\hired.exe    # Windows
```

## Configure

```bash
cp config.example.toml config.toml
$EDITOR config.toml         # set SMTP creds, sender name, default subject/body, cv_path
```

`config.toml` is loaded from the **current working directory** at
startup, so `cd` into the folder where you keep your config and CV
before running `hired`.

For Gmail, generate an [App Password](https://support.google.com/accounts/answer/185833)
and use it as `password`. Port `587` uses STARTTLS, port `465` uses implicit TLS —
both are supported automatically based on the port number.

Drop your CV in the same directory as `cv.pdf` (or point `cv_path` at
any PDF file).

## Run

```bash
hired
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
