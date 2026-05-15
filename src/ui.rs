use crate::app::{App, ComposeField, Screen};
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap,
};
use ratatui::Frame;

// Palette — one accent, one muted, one foreground, plus semantic ok/err/warn.
const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const FG: Color = Color::Gray;
const OK: Color = Color::Green;
const ERR: Color = Color::Red;
const WARN: Color = Color::Yellow;

fn s_accent() -> Style { Style::default().fg(ACCENT) }
fn s_dim() -> Style { Style::default().fg(DIM) }
fn s_fg() -> Style { Style::default().fg(FG) }
fn s_bold() -> Style { Style::default().fg(FG).add_modifier(Modifier::BOLD) }
fn s_bold_accent() -> Style { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) }
fn s_ok() -> Style { Style::default().fg(OK) }
fn s_err() -> Style { Style::default().fg(ERR) }
fn s_warn() -> Style { Style::default().fg(WARN) }

fn frame(area: Rect) -> Rect {
    area.inner(Margin { horizontal: 1, vertical: 0 })
}

fn screen_label(screen: Screen) -> &'static str {
    match screen {
        Screen::Urls => "urls",
        Screen::Scraping => "scraping",
        Screen::Review => "review",
        Screen::Compose => "compose",
        Screen::Sending => "sending",
        Screen::Done => "done",
        Screen::History => "history",
    }
}

fn key_hints(screen: Screen) -> Vec<(&'static str, &'static str)> {
    match screen {
        Screen::Urls => vec![
            ("Ctrl+S", "Scrape"),
            ("Ctrl+L", "Load file"),
            ("H", "History"),
            ("Ctrl+Q", "Quit"),
        ],
        Screen::Scraping => vec![
            ("Ctrl+Q", "Cancel"),
        ],
        Screen::Review => vec![
            ("↑↓", "Select"),
            ("←→", "Pick email"),
            ("Space", "Toggle skip"),
            ("Ctrl+S", "Compose"),
            ("H", "History"),
            ("Esc", "Home"),
        ],
        Screen::Compose => vec![
            ("Tab", "Cycle"),
            ("Ctrl+S", "Send"),
            ("Esc", "Back"),
        ],
        Screen::Sending => vec![
            ("", "Sending…"),
        ],
        Screen::Done => vec![
            ("Enter", "Home"),
            ("Esc", "Home"),
        ],
        Screen::History => vec![
            ("↑↓", "Scroll"),
            ("Esc", "Back"),
        ],
    }
}

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // header
            Constraint::Min(0),     // body
            Constraint::Length(1),  // bottom bar
        ])
        .split(area);

    render_header(f, chunks[0], app);

    let body = frame(chunks[1]);
    match app.screen {
        Screen::Urls => render_urls(f, body, app),
        Screen::Scraping => render_scraping(f, body, app),
        Screen::Review => render_review(f, body, app),
        Screen::Compose => render_compose(f, body, app),
        Screen::Sending => render_sending(f, body, app),
        Screen::Done => render_done(f, body, app),
        Screen::History => render_history(f, body, app),
    }

    render_bottom(f, chunks[2], app);
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let line = Line::from(vec![
        Span::styled(" Hired ", s_bold_accent()),
        Span::styled("·", s_dim()),
        Span::raw(" "),
        Span::styled(screen_label(app.screen), s_bold()),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_bottom(f: &mut Frame, area: Rect, app: &App) {
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::raw(" "));
    let hints = key_hints(app.screen);
    let mut first = true;
    for (k, label) in hints {
        if !first { spans.push(Span::styled("  ", s_dim())); }
        first = false;
        if !k.is_empty() {
            spans.push(Span::styled(format!("[{k}]"), s_bold()));
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(label.to_string(), s_dim()));
    }
    if !app.status.is_empty() {
        spans.push(Span::styled("   ·   ", s_dim()));
        spans.push(Span::styled(app.status.clone(), s_warn()));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ---------- URLs ----------

fn render_urls(f: &mut Frame, area: Rect, app: &mut App) {
    app.urls_input.set_block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(s_dim())
            .title(Span::styled(" target URLs ", s_fg())),
    );
    f.render_widget(&app.urls_input, area);
}

// ---------- Scraping ----------

fn render_scraping(f: &mut Frame, area: Rect, app: &App) {
    let (done, total) = app.scrape_progress;
    let pct = if total == 0 { 0u16 } else { (done * 100 / total).min(100) as u16 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(s_dim())
                .title(Span::styled(" progress ", s_fg())),
        )
        .gauge_style(s_accent())
        .label(format!("{done} / {total}"))
        .percent(pct);
    f.render_widget(gauge, chunks[0]);

    let items: Vec<ListItem> = app.sites.iter().map(|s| {
        let line = if let Some(e) = &s.error {
            Line::from(vec![
                Span::styled("✗ ", s_err()),
                Span::styled(s.url.clone(), s_fg()),
                Span::raw("  "),
                Span::styled(e.clone(), s_dim()),
            ])
        } else {
            Line::from(vec![
                Span::styled("✓ ", s_ok()),
                Span::styled(s.url.clone(), s_fg()),
                Span::raw("  "),
                Span::styled(format!("{} email(s)", s.emails.len()), s_dim()),
            ])
        };
        ListItem::new(line)
    }).collect();

    f.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(s_dim())
                .title(Span::styled(" results ", s_fg())),
        ),
        chunks[1],
    );
}

// ---------- Review ----------

fn render_review(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);

    let items: Vec<ListItem> = app.sites.iter().enumerate().map(|(i, s)| {
        let check = if s.skip {
            Span::styled("○ ", s_dim())
        } else if s.emails.is_empty() {
            Span::styled("✗ ", s_err())
        } else {
            Span::styled("● ", s_ok())
        };
        let label = if !s.company_name.is_empty() { s.company_name.clone() } else { s.url.clone() };
        let style = if i == app.review_idx { s_bold_accent() } else { s_fg() };
        ListItem::new(Line::from(vec![check, Span::styled(label, style)]))
    }).collect();

    let mut state = ListState::default();
    state.select(Some(app.review_idx));
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(s_dim())
                .title(Span::styled(" sites ", s_fg())),
        )
        .highlight_style(Style::default().bg(Color::Rgb(30, 30, 30)));
    f.render_stateful_widget(list, chunks[0], &mut state);

    let detail_lines: Vec<Line> = if let Some(site) = app.sites.get(app.review_idx) {
        let mut ls = vec![Line::from(vec![
            Span::styled("url     ", s_dim()),
            Span::styled(site.url.clone(), s_fg()),
        ])];
        if !site.company_name.is_empty() {
            ls.push(Line::from(vec![
                Span::styled("company ", s_dim()),
                Span::styled(site.company_name.clone(), s_accent()),
            ]));
        }
        ls.push(Line::from(""));

        if let Some(err_msg) = &site.error {
            ls.push(Line::from(vec![
                Span::styled("error   ", s_err()),
                Span::styled(err_msg.clone(), s_dim()),
            ]));
        } else if site.emails.is_empty() {
            ls.push(Line::from(Span::styled("no emails found", s_dim())));
        } else {
            for (i, email) in site.emails.iter().enumerate() {
                let selected = i == site.selected;
                let (marker, style) = if selected {
                    ("▶ ", s_bold_accent())
                } else {
                    ("  ", s_fg())
                };
                let already = app.contacted.contains(&email.to_ascii_lowercase());
                let mut spans = vec![Span::raw(marker), Span::styled(email.clone(), style)];
                if already {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled("⚠ already contacted", s_warn()));
                }
                ls.push(Line::from(spans));
            }
            ls.push(Line::from(""));
            ls.push(Line::from(Span::styled(
                if site.skip { "skipped" } else { "included" },
                if site.skip { s_dim() } else { s_ok() },
            )));
        }
        ls
    } else {
        vec![Line::from(Span::styled("no site selected", s_dim()))]
    };

    f.render_widget(
        Paragraph::new(detail_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(s_dim())
                    .title(Span::styled(" detail ", s_fg())),
            )
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

// ---------- Compose ----------

fn render_compose(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);

    let foc = app.compose_focus;
    app.subject.set_block(card("subject", foc == ComposeField::Subject));
    app.body.set_block(card("body", foc == ComposeField::Body));
    app.cv_path.set_block(card("cv path", foc == ComposeField::CvPath));
    app.transcript_path.set_block(card("transcript path (optional)", foc == ComposeField::TranscriptPath));

    f.render_widget(&app.subject, chunks[0]);
    f.render_widget(&app.body, chunks[1]);
    f.render_widget(&app.cv_path, chunks[2]);
    f.render_widget(&app.transcript_path, chunks[3]);
}

fn card(title: &str, focused: bool) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Span::styled(format!(" {title} "), s_fg()))
        .border_style(if focused { s_accent() } else { s_dim() })
}

// ---------- Sending ----------

fn render_sending(f: &mut Frame, area: Rect, app: &App) {
    let (done, total) = app.send_progress;
    let pct = if total == 0 { 0u16 } else { (done * 100 / total).min(100) as u16 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(s_dim())
                .title(Span::styled(" sending ", s_fg())),
        )
        .gauge_style(s_accent())
        .label(format!("{done} / {total}"))
        .percent(pct);
    f.render_widget(gauge, chunks[0]);

    let items: Vec<ListItem> = app.send_results.iter().map(|r| {
        let line = match &r.status {
            Ok(()) => Line::from(vec![
                Span::styled("✓ ", s_ok()),
                Span::styled(r.email.clone(), s_fg()),
                Span::styled(format!("  {}", r.url), s_dim()),
            ]),
            Err(e) => Line::from(vec![
                Span::styled("✗ ", s_err()),
                Span::styled(r.email.clone(), s_err()),
                Span::styled(format!("  {}  ", r.url), s_dim()),
                Span::styled(e.clone(), s_err()),
            ]),
        };
        ListItem::new(line)
    }).collect();

    f.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(s_dim())
                .title(Span::styled(" log ", s_fg())),
        ),
        chunks[1],
    );
}

// ---------- Done ----------

fn render_done(f: &mut Frame, area: Rect, app: &App) {
    let ok = app.send_results.iter().filter(|r| r.status.is_ok()).count();
    let total = app.send_results.len();
    let failed = total - ok;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(area);

    // Calm one-line summary: a check, "complete", and counts.
    let summary = Line::from(vec![
        Span::styled("✓ ", s_ok()),
        Span::styled("complete", s_bold()),
        Span::styled("   ", s_dim()),
        Span::styled(format!("{ok} sent"), s_ok()),
        Span::styled("   ", s_dim()),
        Span::styled(
            format!("{failed} failed"),
            if failed == 0 { s_dim() } else { s_err() },
        ),
        Span::styled("   ", s_dim()),
        Span::styled(format!("{total} total"), s_dim()),
    ]);
    f.render_widget(Paragraph::new(summary), chunks[0]);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled("results", s_dim()))),
        chunks[1],
    );

    // Compute aligned column widths.
    let max_company = app
        .send_results
        .iter()
        .map(|r| {
            if r.company_name.is_empty() { r.url.as_str() } else { r.company_name.as_str() }
        })
        .map(|s| s.chars().count())
        .max()
        .unwrap_or(0)
        .min(32);

    let items: Vec<ListItem> = app.send_results.iter().map(|r| {
        let company = if r.company_name.is_empty() { r.url.clone() } else { r.company_name.clone() };
        let company_trim: String = company.chars().take(max_company).collect();
        let pad = max_company.saturating_sub(company_trim.chars().count());
        let padding: String = " ".repeat(pad);
        let mark = match &r.status {
            Ok(()) => Span::styled("✓", s_ok()),
            Err(_) => Span::styled("✗", s_err()),
        };
        let email_style = match &r.status {
            Ok(()) => s_fg(),
            Err(_) => s_err(),
        };
        let line = Line::from(vec![
            Span::styled(company_trim, s_fg()),
            Span::raw(padding),
            Span::styled("  →  ", s_dim()),
            Span::styled(r.email.clone(), email_style),
            Span::raw("  "),
            mark,
        ]);
        ListItem::new(line)
    }).collect();

    f.render_widget(List::new(items), chunks[2]);
}

// ---------- History ----------

fn render_history(f: &mut Frame, area: Rect, app: &mut App) {
    if app.history.entries.is_empty() {
        let p = Paragraph::new(Line::from(Span::styled("no sent history yet", s_dim())));
        f.render_widget(p, area);
        return;
    }

    let groups = app.history.by_date_desc();
    let mut items: Vec<ListItem> = Vec::with_capacity(app.history.entries.len() + groups.len());
    let mut sel_logical: usize = 0;
    let mut counter: usize = 0;

    for (date, entries) in &groups {
        items.push(ListItem::new(Line::from(Span::styled(
            format!("  {date}  ─────────────────────────"),
            s_dim(),
        ))));
        for e in entries {
            let line = Line::from(vec![
                Span::raw("    "),
                Span::styled(e.email.clone(), s_fg()),
                Span::styled("  ", s_dim()),
                Span::styled(
                    if e.company_name.is_empty() { e.url.clone() } else { e.company_name.clone() },
                    s_dim(),
                ),
            ]);
            items.push(ListItem::new(line));
            if counter == app.history_idx { sel_logical = items.len() - 1; }
            counter += 1;
        }
    }

    let mut state = ListState::default();
    state.select(Some(sel_logical));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(s_dim())
                .title(Span::styled(" sent history ", s_fg())),
        )
        .highlight_style(Style::default().bg(Color::Rgb(30, 30, 30)));

    f.render_stateful_widget(list, area, &mut state);
}
