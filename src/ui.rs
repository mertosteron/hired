use crate::app::{App, ComposeField, Screen};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap,
};
use ratatui::Frame;

const ACCENT: Color = Color::Cyan;
const MUTED: Color = Color::DarkGray;
const OK: Color = Color::Green;
const ERR: Color = Color::Red;
const WARN: Color = Color::Yellow;

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    render_header(f, chunks[0], app);
    match app.screen {
        Screen::Urls => render_urls(f, chunks[1], app),
        Screen::Scraping => render_scraping(f, chunks[1], app),
        Screen::Review => render_review(f, chunks[1], app),
        Screen::Compose => render_compose(f, chunks[1], app),
        Screen::Sending => render_sending(f, chunks[1], app),
        Screen::Done => render_done(f, chunks[1], app),
    }
    render_status(f, chunks[2], app);
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let title = match app.screen {
        Screen::Urls => "1/4  URLs",
        Screen::Scraping => "Scraping…",
        Screen::Review => "2/4  Review scraped emails",
        Screen::Compose => "3/4  Compose message",
        Screen::Sending => "Sending…",
        Screen::Done => "4/4  Results",
    };
    let header = Paragraph::new(Line::from(vec![
        Span::styled(" Hired ", Style::default().fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(title, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::raw("    "),
        Span::styled(
            "Ctrl+Q quit · F2 next · Esc back",
            Style::default().fg(MUTED),
        ),
    ]))
    .block(Block::default().borders(Borders::BOTTOM));
    f.render_widget(header, area);
}

fn render_status(f: &mut Frame, area: Rect, app: &App) {
    let p = Paragraph::new(app.status.clone())
        .style(Style::default().fg(WARN))
        .block(Block::default().borders(Borders::TOP).title(" status "))
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

fn render_urls(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(5)])
        .split(area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" target URLs (one per line) ")
        .border_style(Style::default().fg(ACCENT));
    app.urls_input.set_block(block);
    f.render_widget(&app.urls_input, chunks[0]);

    let help = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("F2", bold()),
            Span::raw("  start scraping     "),
            Span::styled("Ctrl+L", bold()),
            Span::raw("  load urls.txt     "),
            Span::styled("Ctrl+Q", bold()),
            Span::raw("  quit"),
        ]),
        Line::from(Span::styled(
            "Lines like 'acme.io' are auto-prefixed with https://",
            Style::default().fg(MUTED),
        )),
    ])
    .block(Block::default().borders(Borders::ALL).title(" help "));
    f.render_widget(help, chunks[1]);
}

fn render_scraping(f: &mut Frame, area: Rect, app: &App) {
    let (done, total) = app.scrape_progress;
    let pct = if total == 0 { 0 } else { (done * 100 / total).min(100) };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" progress "))
        .gauge_style(Style::default().fg(ACCENT))
        .label(format!("{done}/{total}"))
        .percent(pct as u16);
    f.render_widget(gauge, chunks[0]);

    let items: Vec<ListItem> = app
        .sites
        .iter()
        .map(|s| {
            let label = if let Some(err) = &s.error {
                Line::from(vec![
                    Span::styled("✗ ", Style::default().fg(ERR)),
                    Span::raw(s.url.clone()),
                    Span::raw("  "),
                    Span::styled(err.clone(), Style::default().fg(MUTED)),
                ])
            } else {
                Line::from(vec![
                    Span::styled("✓ ", Style::default().fg(OK)),
                    Span::raw(s.url.clone()),
                    Span::raw("  "),
                    Span::styled(
                        format!("{} email(s)", s.emails.len()),
                        Style::default().fg(MUTED),
                    ),
                ])
            };
            ListItem::new(label)
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" results "));
    f.render_widget(list, chunks[1]);
}

fn render_review(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    let items: Vec<ListItem> = app
        .sites
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let prefix = if s.skip {
                Span::styled("[ ] ", Style::default().fg(MUTED))
            } else if s.emails.is_empty() {
                Span::styled("[!] ", Style::default().fg(ERR))
            } else {
                Span::styled("[x] ", Style::default().fg(OK))
            };
            let url_style = if i == app.review_idx {
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![prefix, Span::styled(s.url.clone(), url_style)]))
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.review_idx));
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" sites — Space toggles skip ")
                .border_style(Style::default().fg(ACCENT)),
        )
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, chunks[0], &mut state);

    // detail pane
    let detail_lines: Vec<Line> = if let Some(site) = app.sites.get(app.review_idx) {
        let mut ls = vec![
            Line::from(vec![
                Span::styled("URL:   ", Style::default().fg(MUTED)),
                Span::raw(site.url.clone()),
            ]),
            Line::from(""),
        ];
        if let Some(err) = &site.error {
            ls.push(Line::from(vec![
                Span::styled("error: ", Style::default().fg(ERR)),
                Span::raw(err.clone()),
            ]));
        }
        if site.emails.is_empty() {
            ls.push(Line::from(Span::styled(
                "(no candidate emails)",
                Style::default().fg(MUTED),
            )));
        } else {
            ls.push(Line::from(vec![
                Span::styled("emails (←/→ to pick):", Style::default().fg(MUTED)),
            ]));
            for (i, email) in site.emails.iter().enumerate() {
                let marker = if i == site.selected { "● " } else { "○ " };
                let style = if i == site.selected {
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ls.push(Line::from(vec![
                    Span::styled(marker, style),
                    Span::styled(email.clone(), style),
                ]));
            }
            ls.push(Line::from(""));
            ls.push(Line::from(vec![
                Span::styled(
                    if site.skip { "status: skipped" } else { "status: include" },
                    Style::default().fg(if site.skip { MUTED } else { OK }),
                ),
            ]));
        }
        ls
    } else {
        vec![Line::from(Span::styled(
            "no site selected",
            Style::default().fg(MUTED),
        ))]
    };

    let detail = Paragraph::new(detail_lines)
        .block(Block::default().borders(Borders::ALL).title(" detail "))
        .wrap(Wrap { trim: false });
    f.render_widget(detail, chunks[1]);
}

fn render_compose(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);

    let focused_style = Style::default().fg(ACCENT);
    let unfocused_style = Style::default().fg(MUTED);

    fn block(title: &str, focused: bool) -> Block<'_> {
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" {title} "))
            .border_style(if focused {
                Style::default().fg(ACCENT)
            } else {
                Style::default().fg(MUTED)
            })
    }

    app.subject.set_block(block(
        "subject",
        app.compose_focus == ComposeField::Subject,
    ));
    app.body
        .set_block(block("body", app.compose_focus == ComposeField::Body));
    app.cv_path.set_block(block(
        "cv path (PDF required)",
        app.compose_focus == ComposeField::CvPath,
    ));
    app.transcript_path.set_block(block(
        "transcript path (optional PDF)",
        app.compose_focus == ComposeField::TranscriptPath,
    ));

    f.render_widget(&app.subject, chunks[0]);
    f.render_widget(&app.body, chunks[1]);
    f.render_widget(&app.cv_path, chunks[2]);
    f.render_widget(&app.transcript_path, chunks[3]);

    let count = app.sites.iter().filter(|s| !s.skip && !s.emails.is_empty()).count();
    let capped = count.min(app.config.daily_limit);
    let preview = Paragraph::new(Line::from(vec![
        Span::styled("Tab/BackTab", bold()),
        Span::raw(" cycle  "),
        Span::styled("F2", bold()),
        Span::raw(" send  "),
        Span::styled("Esc", bold()),
        Span::raw(" back   "),
        Span::styled(format!("{capped} target(s)"), focused_style),
        Span::raw(if count > app.config.daily_limit {
            format!(" (limit {}/{})", app.config.daily_limit, count)
        } else {
            String::new()
        }),
        Span::raw("  delay: "),
        Span::styled(
            format!("{}-{}s", app.config.send_delay_min_secs, app.config.send_delay_max_secs),
            unfocused_style,
        ),
        Span::raw("  from: "),
        Span::styled(
            format!("{} <{}>", app.config.smtp.from_name, app.config.smtp.from_address),
            unfocused_style,
        ),
    ]))
    .block(Block::default().borders(Borders::ALL).title(" actions "));
    f.render_widget(preview, chunks[4]);
}

fn render_sending(f: &mut Frame, area: Rect, app: &App) {
    let (done, total) = app.send_progress;
    let pct = if total == 0 { 0 } else { (done * 100 / total).min(100) };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" send progress "))
        .gauge_style(Style::default().fg(ACCENT))
        .label(format!("{done}/{total}"))
        .percent(pct as u16);
    f.render_widget(gauge, chunks[0]);

    render_send_log(f, chunks[1], app);
}

fn render_done(f: &mut Frame, area: Rect, app: &App) {
    render_send_log(f, area, app);
}

fn render_send_log(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .send_results
        .iter()
        .map(|r| {
            let line = match &r.status {
                Ok(()) => Line::from(vec![
                    Span::styled("✓ ", Style::default().fg(OK)),
                    Span::styled(r.email.clone(), Style::default().fg(OK).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("  ({})", r.url), Style::default().fg(MUTED)),
                ]),
                Err(e) => Line::from(vec![
                    Span::styled("✗ ", Style::default().fg(ERR)),
                    Span::styled(r.email.clone(), Style::default().fg(ERR)),
                    Span::styled(format!("  ({})  ", r.url), Style::default().fg(MUTED)),
                    Span::styled(e.clone(), Style::default().fg(ERR)),
                ]),
            };
            ListItem::new(line)
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" send log "));
    f.render_widget(list, area);
}

fn bold() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

#[allow(dead_code)]
fn _silence_alignment_unused() {
    let _ = Alignment::Center;
}
