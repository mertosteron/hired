use crate::error::BotError;
use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{Html, Selector};
use std::collections::HashSet;
use std::time::Duration;
use url::Url;

static EMAIL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,24}").unwrap()
});

const CONTACT_KEYWORDS: &[&str] = &[
    // English
    "contact", "kontakt", "about", "career", "careers", "jobs", "join",
    "hiring", "hr", "people", "team", "recruit", "talent", "apply",
    // Turkish
    "iletisim", "iletişim", "hakkimizda", "hakkında", "kariyer",
    "bize-ulasin", "bize-ulaşın", "insan-kaynaklari", "staj", "intern",
    "başvuru", "basvuru",
];

const JUNK_NEEDLES: &[&str] = &[
    "example.com",
    "yourdomain",
    "yourcompany",
    "domain.com",
    "sentry.io",
    "sentry-next.wixpress.com",
    "wixpress.com",
    "u003c",
    "u002",
];

fn looks_like_image(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".svg", ".gif", ".webp", ".ico", ".bmp"]
        .iter()
        .any(|ext| l.ends_with(ext))
}

fn is_junk(addr: &str) -> bool {
    let lc = addr.to_ascii_lowercase();
    if looks_like_image(&lc) {
        return true;
    }
    JUNK_NEEDLES.iter().any(|n| lc.contains(n))
}

/// Stage 1 + 4: mailto: href links + regex over full HTML text.
fn extract_emails_from_text(html: &str) -> HashSet<String> {
    let mut out: HashSet<String> = HashSet::new();
    let doc = Html::parse_document(html);

    // Stage 4a: <a href="mailto:...">
    if let Ok(sel) = Selector::parse(r#"a[href^="mailto:"]"#) {
        for a in doc.select(&sel) {
            if let Some(href) = a.value().attr("href") {
                let raw = href.trim_start_matches("mailto:");
                let addr = raw.split(['?', '#']).next().unwrap_or("").trim();
                if !addr.is_empty() && EMAIL_RE.is_match(addr) && !is_junk(addr) {
                    out.insert(addr.to_string());
                }
            }
        }
    }

    // Stage 1: regex over raw HTML (catches obfuscated/encoded emails in JS, comments, etc.)
    for m in EMAIL_RE.find_iter(html) {
        let s = m.as_str();
        if !is_junk(s) {
            out.insert(s.to_string());
        }
    }

    out
}

/// Stage 5: scan ALL element attributes for email-like values (data-email, etc.)
fn extract_emails_from_attrs(html: &str) -> HashSet<String> {
    let mut out: HashSet<String> = HashSet::new();
    let doc = Html::parse_document(html);
    if let Ok(sel) = Selector::parse("*") {
        for el in doc.select(&sel) {
            for (_name, val) in el.value().attrs() {
                if val.contains('@') {
                    for m in EMAIL_RE.find_iter(val) {
                        let s = m.as_str();
                        if !is_junk(s) {
                            out.insert(s.to_string());
                        }
                    }
                }
            }
        }
    }
    out
}

/// Stage 3b: scan <script> tag bodies for emails (common in JS-rendered sites).
fn extract_emails_from_scripts(html: &str) -> HashSet<String> {
    let mut out: HashSet<String> = HashSet::new();
    let doc = Html::parse_document(html);
    if let Ok(sel) = Selector::parse("script") {
        for script in doc.select(&sel) {
            let text = script.text().collect::<String>();
            if text.contains('@') {
                for m in EMAIL_RE.find_iter(&text) {
                    let s = m.as_str();
                    if !is_junk(s) {
                        out.insert(s.to_string());
                    }
                }
            }
        }
    }
    out
}

/// Collect all email candidates from a single HTML page (all 5 stages).
fn scan_page(html: &str) -> HashSet<String> {
    let mut found = HashSet::new();
    found.extend(extract_emails_from_text(html));
    found.extend(extract_emails_from_attrs(html));
    found.extend(extract_emails_from_scripts(html));
    found
}

fn extract_contact_links(html: &str, base: &Url) -> Vec<Url> {
    let doc = Html::parse_document(html);
    let mut out: Vec<Url> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let Ok(sel) = Selector::parse("a[href]") else {
        return out;
    };

    for a in doc.select(&sel) {
        let Some(href) = a.value().attr("href") else { continue };
        if href.starts_with("mailto:") || href.starts_with("tel:") || href.starts_with('#') {
            continue;
        }
        let text = a.text().collect::<String>().to_lowercase();
        let href_low = href.to_lowercase();
        let is_contact = CONTACT_KEYWORDS
            .iter()
            .any(|k| href_low.contains(k) || text.contains(k));
        if !is_contact {
            continue;
        }
        if let Ok(url) = base.join(href) {
            if !url.scheme().starts_with("http") {
                continue;
            }
            if url.host_str() != base.host_str() {
                continue;
            }
            let key = url.as_str().to_string();
            if seen.insert(key) {
                out.push(url);
            }
        }
        if out.len() >= 8 {
            break;
        }
    }
    out
}

fn rank(addr: &str) -> i32 {
    let lc = addr.to_ascii_lowercase();
    let local = lc.split('@').next().unwrap_or("");
    let mut score = 0;
    for good in [
        "hr", "jobs", "careers", "career", "recruit", "hiring",
        "people", "talent", "kariyer", "ik", "staj", "intern",
    ] {
        if local.contains(good) {
            score -= 100;
        }
    }
    for good in ["contact", "info", "hello", "office", "iletisim"] {
        if local.contains(good) {
            score -= 30;
        }
    }
    if local == "no-reply" || local == "noreply" || local.starts_with("donot") {
        score += 100;
    }
    score
}

pub async fn scrape_emails_for_url(input: &str) -> Result<Vec<String>, BotError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(BotError::InvalidUrl(input.into()));
    }
    let normalized = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{}", trimmed)
    };
    let url = Url::parse(&normalized).map_err(|_| BotError::InvalidUrl(input.into()))?;

    let client = reqwest::Client::builder()
        .user_agent(concat!(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 ",
            "(KHTML, like Gecko) Chrome/124.0 Safari/537.36 HiredBot/0.1"
        ))
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;

    let mut found: HashSet<String> = HashSet::new();

    // Stage 1-2: main page full scan
    let resp = client.get(url.as_str()).send().await?;
    let final_url = resp.url().clone();
    let body = resp.text().await?;
    found.extend(scan_page(&body));

    // Stage 3: follow contact/about/career sub-pages (up to 8)
    let contact_links = extract_contact_links(&body, &final_url);
    for link in contact_links {
        if let Ok(r) = client.get(link.as_str()).send().await {
            if let Ok(text) = r.text().await {
                found.extend(scan_page(&text));
            }
        }
    }

    let mut emails: Vec<String> = found.into_iter().collect();
    emails.sort_by(|a, b| {
        let ra = rank(a);
        let rb = rank(b);
        ra.cmp(&rb).then_with(|| a.cmp(b))
    });

    if emails.is_empty() {
        return Err(BotError::NoEmails(input.to_string()));
    }
    Ok(emails)
}
