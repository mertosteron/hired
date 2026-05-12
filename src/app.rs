use crate::config::Config;
use crate::mailer;
use crate::scraper;
use crate::ui;
use anyhow::Result;
use chrono::{Local, Timelike};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use rand::Rng;
use ratatui::backend::Backend;
use ratatui::Terminal;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tui_textarea::TextArea;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Urls,
    Scraping,
    Review,
    Compose,
    Sending,
    Done,
}

#[derive(Debug, Clone)]
pub struct ScrapedSite {
    pub url: String,
    pub emails: Vec<String>,
    pub selected: usize,
    pub skip: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SendResult {
    pub url: String,
    pub email: String,
    pub status: Result<(), String>,
}

#[derive(Debug)]
pub enum BgEvent {
    ScrapeOne(ScrapedSite),
    ScrapeDone,
    SendOne(SendResult),
    SendDone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeField {
    Subject,
    Body,
    CvPath,
    TranscriptPath,
}

impl ComposeField {
    fn next(self) -> Self {
        match self {
            ComposeField::Subject => ComposeField::Body,
            ComposeField::Body => ComposeField::CvPath,
            ComposeField::CvPath => ComposeField::TranscriptPath,
            ComposeField::TranscriptPath => ComposeField::Subject,
        }
    }

    fn prev(self) -> Self {
        // prev = next 3 times in a 4-field cycle
        self.next().next().next()
    }
}

pub struct App {
    pub config: Config,
    pub screen: Screen,

    pub urls_input: TextArea<'static>,
    pub subject: TextArea<'static>,
    pub body: TextArea<'static>,
    pub cv_path: TextArea<'static>,
    pub transcript_path: TextArea<'static>,
    pub compose_focus: ComposeField,

    pub sites: Vec<ScrapedSite>,
    pub review_idx: usize,

    pub send_results: Vec<SendResult>,

    pub status: String,
    pub bg_rx: Option<mpsc::UnboundedReceiver<BgEvent>>,

    pub scrape_progress: (usize, usize),
    pub send_progress: (usize, usize),

    pub should_quit: bool,
}

impl App {
    pub fn new(config: Config, startup_warning: Option<String>) -> Self {
        let mut urls_input = TextArea::default();
        urls_input.set_placeholder_text("Paste one URL per line, e.g.\nhttps://example.com\nacme.io");

        let mut subject = TextArea::from(vec![config.default_subject.clone()]);
        subject.set_cursor_line_style(Default::default());

        let body = TextArea::from(
            config
                .default_body
                .lines()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
        );

        let mut cv_path = TextArea::from(vec![config.cv_path.clone()]);
        cv_path.set_cursor_line_style(Default::default());

        let mut transcript_path = TextArea::from(vec![config.transcript_path.clone()]);
        transcript_path.set_cursor_line_style(Default::default());

        Self {
            config,
            screen: Screen::Urls,
            urls_input,
            subject,
            body,
            cv_path,
            transcript_path,
            compose_focus: ComposeField::Subject,
            sites: Vec::new(),
            review_idx: 0,
            send_results: Vec::new(),
            status: startup_warning.unwrap_or_else(|| {
                "Ready. Paste URLs (one per line). Press F2 to scrape, Ctrl+Q to quit.".into()
            }),
            bg_rx: None,
            scrape_progress: (0, 0),
            send_progress: (0, 0),
            should_quit: false,
        }
    }

    pub async fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        let tick = Duration::from_millis(80);
        let mut last_tick = Instant::now();

        while !self.should_quit {
            terminal.draw(|f| ui::render(f, self))?;

            let timeout = tick
                .checked_sub(last_tick.elapsed())
                .unwrap_or(Duration::from_millis(0));

            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat {
                        self.handle_key(key);
                    }
                }
            }

            self.drain_background();

            if last_tick.elapsed() >= tick {
                last_tick = Instant::now();
            }
        }
        Ok(())
    }

    fn drain_background(&mut self) {
        let Some(rx) = self.bg_rx.as_mut() else { return };
        loop {
            match rx.try_recv() {
                Ok(BgEvent::ScrapeOne(site)) => {
                    self.scrape_progress.0 += 1;
                    self.sites.push(site);
                }
                Ok(BgEvent::ScrapeDone) => {
                    self.bg_rx = None;
                    self.screen = Screen::Review;
                    self.review_idx = 0;
                    let total = self.sites.len();
                    let with_emails = self.sites.iter().filter(|s| !s.emails.is_empty()).count();
                    self.status = format!(
                        "Scraped {total} site(s); {with_emails} have emails. ←/→ pick email, Space toggles skip, F2 continues."
                    );
                    break;
                }
                Ok(BgEvent::SendOne(res)) => {
                    self.send_progress.0 += 1;
                    self.send_results.push(res);
                }
                Ok(BgEvent::SendDone) => {
                    self.bg_rx = None;
                    self.screen = Screen::Done;
                    let ok = self
                        .send_results
                        .iter()
                        .filter(|r| r.status.is_ok())
                        .count();
                    let log_path = self.write_csv_log();
                    self.status = format!(
                        "Done. {}/{} sent successfully. Log: {}  q/Esc to quit.",
                        ok,
                        self.send_results.len(),
                        log_path,
                    );
                    break;
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.bg_rx = None;
                    break;
                }
            }
        }
    }

    fn write_csv_log(&self) -> String {
        use std::io::Write;
        let date = Local::now().format("%Y%m%d").to_string();
        let path = format!("send_log_{date}.csv");
        let write = || -> std::io::Result<()> {
            let exists = std::path::Path::new(&path).exists();
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)?;
            if !exists {
                writeln!(f, "timestamp,url,email,status,error")?;
            }
            let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            for r in &self.send_results {
                let (status, error) = match &r.status {
                    Ok(()) => ("ok", String::new()),
                    Err(e) => ("error", e.replace(',', ";")),
                };
                writeln!(f, "{now},{},{},{status},{error}", r.url, r.email)?;
            }
            Ok(())
        };
        if write().is_ok() { path } else { "(log write failed)".into() }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }
        match self.screen {
            Screen::Urls => self.handle_urls_key(key),
            Screen::Scraping => self.handle_busy_key(key),
            Screen::Review => self.handle_review_key(key),
            Screen::Compose => self.handle_compose_key(key),
            Screen::Sending => self.handle_busy_key(key),
            Screen::Done => self.handle_done_key(key),
        }
    }

    fn handle_busy_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.status = "Working… please wait. Ctrl+Q to force quit.".into();
        }
    }

    fn handle_urls_key(&mut self, key: KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::F(2), _)
            | (KeyCode::Char('s'), KeyModifiers::CONTROL)
            | (KeyCode::Enter, KeyModifiers::CONTROL) => {
                self.start_scrape();
            }
            (KeyCode::Char('l'), KeyModifiers::CONTROL) => {
                match std::fs::read_to_string("urls.txt") {
                    Ok(s) => {
                        self.urls_input =
                            TextArea::from(s.lines().map(|x| x.to_string()).collect::<Vec<_>>());
                        self.status = "Loaded urls.txt".into();
                    }
                    Err(e) => self.status = format!("Cannot read urls.txt: {e}"),
                }
            }
            (KeyCode::Esc, _) => {
                self.should_quit = true;
            }
            _ => {
                self.urls_input.input(key);
            }
        }
    }

    fn handle_review_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.review_idx > 0 {
                    self.review_idx -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.review_idx + 1 < self.sites.len() {
                    self.review_idx += 1;
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if let Some(site) = self.sites.get_mut(self.review_idx) {
                    if !site.emails.is_empty() && site.selected > 0 {
                        site.selected -= 1;
                    }
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if let Some(site) = self.sites.get_mut(self.review_idx) {
                    if !site.emails.is_empty() && site.selected + 1 < site.emails.len() {
                        site.selected += 1;
                    }
                }
            }
            KeyCode::Char(' ') => {
                if let Some(site) = self.sites.get_mut(self.review_idx) {
                    site.skip = !site.skip;
                }
            }
            KeyCode::F(2) | KeyCode::Enter => {
                let any_picked = self
                    .sites
                    .iter()
                    .any(|s| !s.skip && !s.emails.is_empty());
                if !any_picked {
                    self.status = "Nothing to send — all sites skipped or empty.".into();
                    return;
                }
                self.screen = Screen::Compose;
                self.status =
                    "Tab/BackTab cycle fields. Edit subject/body/CV/transcript. F2 to send. Esc back.".into();
            }
            KeyCode::Esc => {
                self.screen = Screen::Urls;
                self.sites.clear();
                self.scrape_progress = (0, 0);
                self.status = "Back to URL input.".into();
            }
            _ => {}
        }
    }

    fn handle_compose_key(&mut self, key: KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::F(2), _) => {
                self.start_send();
                return;
            }
            (KeyCode::Esc, _) => {
                self.screen = Screen::Review;
                self.status = "Back to review.".into();
                return;
            }
            (KeyCode::Tab, _) => {
                self.compose_focus = self.compose_focus.next();
                return;
            }
            (KeyCode::BackTab, _) => {
                self.compose_focus = self.compose_focus.prev();
                return;
            }
            _ => {}
        }
        let is_single_line = matches!(
            self.compose_focus,
            ComposeField::Subject | ComposeField::CvPath | ComposeField::TranscriptPath
        );
        if is_single_line && key.code == KeyCode::Enter {
            return;
        }
        let target: &mut TextArea<'static> = match self.compose_focus {
            ComposeField::Subject => &mut self.subject,
            ComposeField::Body => &mut self.body,
            ComposeField::CvPath => &mut self.cv_path,
            ComposeField::TranscriptPath => &mut self.transcript_path,
        };
        target.input(key);
    }

    fn handle_done_key(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
            self.should_quit = true;
        }
    }

    fn start_scrape(&mut self) {
        let urls: Vec<String> = self
            .urls_input
            .lines()
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if urls.is_empty() {
            self.status = "No URLs to scrape.".into();
            return;
        }

        self.sites.clear();
        self.scrape_progress = (0, urls.len());
        self.screen = Screen::Scraping;
        self.status = format!("Scraping {} site(s)…", urls.len());

        let (tx, rx) = mpsc::unbounded_channel();
        self.bg_rx = Some(rx);

        tokio::spawn(async move {
            for url in urls {
                let site = match scraper::scrape_emails_for_url(&url).await {
                    Ok(emails) => ScrapedSite {
                        url: url.clone(),
                        emails,
                        selected: 0,
                        skip: false,
                        error: None,
                    },
                    Err(e) => ScrapedSite {
                        url: url.clone(),
                        emails: Vec::new(),
                        selected: 0,
                        skip: true,
                        error: Some(e.to_string()),
                    },
                };
                let _ = tx.send(BgEvent::ScrapeOne(site));
            }
            let _ = tx.send(BgEvent::ScrapeDone);
        });
    }

    fn start_send(&mut self) {
        let subject = self.subject.lines().join(" ").trim().to_string();
        let body = self.body.lines().join("\n");
        let cv_path = self
            .cv_path
            .lines()
            .first()
            .cloned()
            .unwrap_or_default()
            .trim()
            .to_string();
        let transcript_path = self
            .transcript_path
            .lines()
            .first()
            .cloned()
            .unwrap_or_default()
            .trim()
            .to_string();

        if subject.is_empty() {
            self.status = "Subject is empty.".into();
            return;
        }
        if !std::path::Path::new(&cv_path).is_file() {
            self.status = format!("CV file not found: {cv_path}");
            return;
        }
        if self.config.smtp.username.is_empty() || self.config.smtp.password.is_empty() {
            self.status = "SMTP credentials missing — fill config.toml.".into();
            return;
        }

        // Time window check
        let hour = Local::now().hour();
        if hour < self.config.send_window_start || hour >= self.config.send_window_end {
            self.status = format!(
                "Outside send window ({}:00–{}:00). Emails are sent only during these hours to maximize read rates.",
                self.config.send_window_start, self.config.send_window_end
            );
            return;
        }

        let all_jobs: Vec<(String, String)> = self
            .sites
            .iter()
            .filter(|s| !s.skip && !s.emails.is_empty())
            .map(|s| (s.url.clone(), s.emails[s.selected].clone()))
            .collect();

        if all_jobs.is_empty() {
            self.status = "No targets selected.".into();
            return;
        }

        // Daily limit
        let limit = self.config.daily_limit;
        let capped = all_jobs.len() > limit;
        let jobs: Vec<(String, String)> = all_jobs.into_iter().take(limit).collect();

        self.send_results.clear();
        self.send_progress = (0, jobs.len());
        self.screen = Screen::Sending;
        self.status = if capped {
            format!(
                "Sending {} email(s) (capped at daily limit {})… delay: {}-{}s per email.",
                jobs.len(), limit,
                self.config.send_delay_min_secs, self.config.send_delay_max_secs,
            )
        } else {
            format!(
                "Sending {} email(s)… delay: {}-{}s per email.",
                jobs.len(),
                self.config.send_delay_min_secs, self.config.send_delay_max_secs,
            )
        };

        let smtp = self.config.smtp.clone();
        let delay_min = self.config.send_delay_min_secs;
        let delay_max = self.config.send_delay_max_secs;
        let (tx, rx) = mpsc::unbounded_channel();
        self.bg_rx = Some(rx);

        tokio::spawn(async move {
            for (i, (url, email)) in jobs.iter().enumerate() {
                let res = mailer::send_email(&smtp, email, &subject, &body, &cv_path, &transcript_path).await;
                let send_result = SendResult {
                    url: url.clone(),
                    email: email.clone(),
                    status: res.map_err(|e| e.to_string()),
                };
                let _ = tx.send(BgEvent::SendOne(send_result));
                if i + 1 < jobs.len() {
                    // Compute delay before await so ThreadRng (non-Send) is dropped.
                    let secs = if delay_max > delay_min {
                        rand::thread_rng().gen_range(delay_min..=delay_max)
                    } else {
                        delay_min
                    };
                    tokio::time::sleep(Duration::from_secs(secs)).await;
                }
            }
            let _ = tx.send(BgEvent::SendDone);
        });
    }
}
