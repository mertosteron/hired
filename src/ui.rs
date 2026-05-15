use crate::app::{App, ComposeField, Screen};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap,
};
use ratatui::Frame;

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const FG: Color = Color::White;
const OK: Color = Color::Green;
const ERR: Color = Color::Red;
const WARN: Color = Color::Yellow;

fn accent() -> Style { Style::default().fg(ACCENT) }
fn dim() -> Style { Style::default().fg(DIM) }
fn bold_accent() -> Style { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) }
fn bold() -> Style { Style::default().add_modifier(Modifier::BOLD) }
fn ok() -> Style { Style::default().fg(OK) }
fn err() -> Style { Style::default().fg(ERR) }

fn card(title: &str, focused: bool) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(format!(" {title} "))
        .border_style(if focused { accent() } else { dim() })
}

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
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
    }

    render_status(f, chunks[2], app);
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let step = match app.screen {
        Screen::Urls => "  urls",
        Screen::Scraping => "  scraping",
        Screen::Review => "  review",
        Screen::Compose => "  compose",
        Screen::Sending => "  sending",
    };

    let hints = match app.screen {
        Screen::Urls => "Ctrl+S scrape · Ctrl+L load file · Ctrl+Q quit",
        Screen::Scraping => "working… Ctrl+Q force quit",
        Screen::Review => "↑/↓ nav · ←/→ pick email · Space skip · Ctrl+S next · Esc back",
        Screen::Compose => "Tab cycle · Ctrl+S send · Esc back",
        Screen::Sending => "sending… please wait",
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(" Hired", bold_accent()),
        Span::styled(step, Style::default().fg(FG).add_modifier(Modifier::BOLD)),
        Span::raw("   "),
        Span::styled(hints, dim()),
    ]))
    .block(Block::default().borders(Borders::BOTTOM).border_style(dim()));
    f.render_widget(header, area);
}

fn render_status(f: &mut Frame, area: Rect, app: &App) {
    let p = Paragraph::new(app.status.clone())
        .style(Style::default().fg(WARN))
        .block(Block::default().borders(Borders::TOP).border_style(dim()))
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

fn render_urls(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(4)])
        .split(area);

    app.urls_input.set_block(card("target URLs — one per line, or paste a directory URL", true));
    f.render_widget(&app.urls_input, chunks[0]);

    let help = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Ctrl+S", bold()),
            Span::raw(" start scraping   "),
            Span::styled("Ctrl+L", bold()),
            Span::raw(" load urls.txt   "),
            Span::styled("Ctrl+Q", bold()),
            Span::raw(" quit"),
        ]),
        Line::from(Span::styled(
            "Directory URLs listing multiple companies are auto-expanded.",
            dim(),
        )),
    ])
    .block(Block::default().borders(Borders::TOP).border_style(dim()));
    f.render_widget(help, chunks[1]);
}

fn render_scraping(f: &mut Frame, area: Rect, app: &App) {
    let (done, total) = app.scrape_progress;
    let pct = if total == 0 { 0u16 } else { (done * 100 / total).min(100) as u16 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let gauge = Gauge::default()
        .block(card("progress", false))
        .gauge_style(accent())
        .label(format!("{done} / {total}"))
        .percent(pct);
    f.render_widget(gauge, chunks[0]);

    let items: Vec<ListItem> = app.sites.iter().map(|s| {
        let line = if let Some(e) = &s.error {
            Line::from(vec![
                Span::styled("✗ ", err()),
                Span::raw(s.url.clone()),
                Span::raw("  "),
                Span::styled(e.clone(), dim()),
            ])
        } else {
            Line::from(vec![
                Span::styled("✓ ", ok()),
                Span::raw(s.url.clone()),
                Span::raw("  "),
                Span::styled(format!("{} email(s)", s.emails.len()), dim()),
            ])
        };
        ListItem::new(line)
    }).collect();

    f.render_widget(
        List::new(items).block(card("results", false)),
        chunks[1],
    );
}

fn render_review(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);

    let items: Vec<ListItem> = app.sites.iter().enumerate().map(|(i, s)| {
        let check = if s.skip {
            Span::styled("○  ", dim())
        } else if s.emails.is_empty() {
            Span::styled("✗  ", err())
        } else {
            Span::styled("●  ", ok())
        };
        let name_style = if i == app.review_idx { bold_accent() } else { Style::default().fg(FG) };
        let label = if !s.company_name.is_empty() {
            s.company_name.clone()
        } else {
            s.url.clone()
        };
        ListItem::new(Line::from(vec![check, Span::styled(label, name_style)]))
    }).collect();

    let mut state = ListState::default();
    state.select(Some(app.review_idx));
    let list = List::new(items)
        .block(card("sites  —  Space to skip/include", true))
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("  ");
    f.render_stateful_widget(list, chunks[0], &mut state);

    // Detail pane
    let detail_lines: Vec<Line> = if let Some(site) = app.sites.get(app.review_idx) {
        let mut ls = vec![
            Line::from(vec![
                Span::styled("url     ", dim()),
                Span::styled(site.url.clone(), Style::default().fg(FG)),
            ]),
        ];
        if !site.company_name.is_empty() {
            ls.push(Line::from(vec![
                Span::styled("company ", dim()),
                Span::styled(site.company_name.clone(), accent()),
            ]));
        }
        ls.push(Line::from(""));

        if let Some(err_msg) = &site.error {
            ls.push(Line::from(vec![
                Span::styled("error   ", err()),
                Span::styled(err_msg.clone(), dim()),
            ]));
        } else if site.emails.is_empty() {
            ls.push(Line::from(Span::styled("no emails found", dim())));
        } else {
            ls.push(Line::from(Span::styled("emails  ←/→ to pick", dim())));
            for (i, email) in site.emails.iter().enumerate() {
                let (marker, style) = if i == site.selected {
                    ("▶ ", bold_accent())
                } else {
                    ("  ", Style::default().fg(DIM))
                };
                ls.push(Line::from(vec![
                    Span::raw(marker),
                    Span::styled(email.clone(), style),
                ]));
            }
            ls.push(Line::from(""));
            ls.push(Line::from(Span::styled(
                if site.skip { "skipped" } else { "included" },
                if site.skip { dim() } else { ok() },
            )));
        }
        ls
    } else {
        vec![Line::from(Span::styled("no site selected", dim()))]
    };

    f.render_widget(
        Paragraph::new(detail_lines)
            .block(card("detail", false))
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
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

    let foc = app.compose_focus;
    app.subject.set_block(card("subject", foc == ComposeField::Subject));
    app.body.set_block(card("body  (\"Dear [Company],\" is prepended automatically)", foc == ComposeField::Body));
    app.cv_path.set_block(card("cv path — PDF required", foc == ComposeField::CvPath));
    app.transcript_path.set_block(card("transcript path — optional PDF", foc == ComposeField::TranscriptPath));

    f.render_widget(&app.subject, chunks[0]);
    f.render_widget(&app.body, chunks[1]);
    f.render_widget(&app.cv_path, chunks[2]);
    f.render_widget(&app.transcript_path, chunks[3]);

    let count = app.sites.iter().filter(|s| !s.skip && !s.emails.is_empty()).count();
    let capped = count.min(app.config.daily_limit);
    let info = Paragraph::new(Line::from(vec![
        Span::styled("Ctrl+S", bold()),
        Span::raw(" send   "),
        Span::styled("Tab", bold()),
        Span::raw(" cycle   "),
        Span::styled("Esc", bold()),
        Span::raw(" back   "),
        Span::raw("  "),
        Span::styled(format!("{capped} target(s)", ), accent()),
        if count > app.config.daily_limit {
            Span::styled(format!(" (limit {}/{})", app.config.daily_limit, count), dim())
        } else {
            Span::raw("")
        },
        Span::raw("   window "),
        Span::styled(
            format!("{}:00–{}:00 {}", app.config.send_window_start, app.config.send_window_end, app.config.timezone),
            dim(),
        ),
    ]))
    .block(Block::default().borders(Borders::TOP).border_style(dim()));
    f.render_widget(info, chunks[4]);
}

fn render_sending(f: &mut Frame, area: Rect, app: &App) {
    let (done, total) = app.send_progress;
    let pct = if total == 0 { 0u16 } else { (done * 100 / total).min(100) as u16 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let gauge = Gauge::default()
        .block(card("sending", false))
        .gauge_style(accent())
        .label(format!("{done} / {total}"))
        .percent(pct);
    f.render_widget(gauge, chunks[0]);

    render_send_log(f, chunks[1], app);
}

fn render_send_log(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app.send_results.iter().map(|r| {
        let line = match &r.status {
            Ok(()) => Line::from(vec![
                Span::styled("✓ ", ok()),
                Span::styled(r.email.clone(), Style::default().fg(OK).add_modifier(Modifier::BOLD)),
                Span::styled(format!("  {}", r.url), dim()),
            ]),
            Err(e) => Line::from(vec![
                Span::styled("✗ ", err()),
                Span::styled(r.email.clone(), err()),
                Span::styled(format!("  {}  ", r.url), dim()),
                Span::styled(e.clone(), err()),
            ]),
        };
        ListItem::new(line)
    }).collect();

    f.render_widget(
        List::new(items).block(card("send log", false)),
        area,
    );
}
