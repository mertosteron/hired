//! Verify-scraper harness.
//!
//! Runs the production scraper against a fixed set of Turkish tech companies
//! and prints a boxed, per-site report plus a summary line. Used to gate
//! Enhancement 1 changes — pass criterion is `argenova.com.tr` returning at
//! least one email.
//!
//! Run with `cargo run --bin verify`.

use std::time::Instant;

use hired::scraper::{scrape_with_provenance, EmailCategory, EmailFinding};

/// Fixed test corpus — five Turkish tech companies in different architectures.
const TARGETS: &[(&str, &str)] = &[
    ("argenova.com.tr", "https://argenova.com.tr/"),
    ("getir.com",       "https://getir.com/"),
    ("param.com.tr",    "https://param.com.tr/"),
    ("macellan.com.tr", "https://macellan.com.tr/"),
    ("logo.com.tr",     "https://logo.com.tr/"),
];

#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!();
    println!("hired :: scraper verification");
    println!("================================");

    let mut passed = 0usize;
    for (label, url) in TARGETS {
        let started = Instant::now();
        let res = scrape_with_provenance(url).await;
        let elapsed_ms = started.elapsed().as_millis();

        match res {
            Ok(report) => {
                passed += 1;
                print_box_success(label, url, &report.findings, elapsed_ms);
            }
            Err(e) => print_box_failure(label, url, &e.to_string(), elapsed_ms),
        }
        println!();
    }

    println!("================================");
    println!("summary: {passed}/{} site(s) returned at least one email.", TARGETS.len());

    if passed != TARGETS.len() {
        std::process::exit(1);
    }
}

fn print_box_success(label: &str, url: &str, findings: &[EmailFinding], elapsed_ms: u128) {
    let header = format!("Şirket: {label}");
    let url_line = format!("URL:    {url}");
    let status = "Durum:  ✓ BAŞARILI".to_string();

    let mut lines: Vec<String> = vec![header, url_line, status];

    for (i, f) in findings.iter().take(8).enumerate() {
        let prefix = if i == 0 { "Birincil:" } else { "         " };
        let cat = match f.category {
            EmailCategory::Careers => "kariyer",
            EmailCategory::General => "genel",
            EmailCategory::Other => "diğer",
        };
        lines.push(format!(
            "{prefix} {email}  [{cat}]",
            email = f.email,
        ));
        let src_short = shorten_url(&f.source_url, 56);
        lines.push(format!("           → {src_short}  ({})", f.method.as_str()));
    }
    if findings.len() > 8 {
        lines.push(format!("         (+{} more)", findings.len() - 8));
    }
    lines.push(format!("Süre:   {elapsed_ms}ms"));

    print_box(&lines);
}

fn print_box_failure(label: &str, url: &str, err: &str, elapsed_ms: u128) {
    let lines: Vec<String> = vec![
        format!("Şirket: {label}"),
        format!("URL:    {url}"),
        "Durum:  ✗ BAŞARISIZ".to_string(),
        format!("Hata:   {err}"),
        format!("Süre:   {elapsed_ms}ms"),
    ];
    print_box(&lines);
}

fn print_box(lines: &[String]) {
    let max_width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let inner = max_width + 2;
    let top = format!("┌{}┐", "─".repeat(inner));
    let bot = format!("└{}┘", "─".repeat(inner));
    println!("{top}");
    for line in lines {
        let pad = max_width.saturating_sub(line.chars().count());
        println!("│ {line}{} │", " ".repeat(pad));
    }
    println!("{bot}");
}

fn shorten_url(s: &str, max: usize) -> String {
    if s.chars().count() <= max { return s.to_string(); }
    let half = max / 2 - 2;
    let head: String = s.chars().take(half).collect();
    let tail: String = s.chars().rev().take(half).collect::<String>().chars().rev().collect();
    format!("{head}…{tail}")
}
