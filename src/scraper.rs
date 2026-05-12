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
    "contact", "kontakt", "about", "career", "careers", "jobs", "join",
    "hiring", "hr", "people", "team", "recruit", "talent", "apply",
    "iletisim", "iletişim", "hakkimizda", "hakkında", "kariyer",
    "bize-ulasin", "bize-ulaşın", "insan-kaynaklari", "staj", "intern",
    "başvuru", "basvuru",
];

const JUNK_NEEDLES: &[&str] = &[
    "example.com", "yourdomain", "yourcompany", "domain.com",
    "sentry.io", "sentry-next.wixpress.com", "wixpress.com",
    "u003c", "u002",
];

const SOCIAL_DOMAINS: &[&str] = &[
    "twitter.com", "x.com", "facebook.com", "linkedin.com",
    "instagram.com", "youtube.com", "github.com", "medium.com",
    "t.me", "telegram.org", "discord.com", "discord.gg",
    "reddit.com", "google.com", "apple.com", "cloudflare.com",
    "amazonaws.com", "googleusercontent.com", "gstatic.com",
    "coinmarketcap.com", "coingecko.com", "crunchbase.com",
    "angellist.com", "pitchbook.com",
];

fn looks_like_image(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".svg", ".gif", ".webp", ".ico", ".bmp"]
        .iter()
        .any(|ext| l.ends_with(ext))
}

fn is_junk(addr: &str) -> bool {
    let lc = addr.to_ascii_lowercase();
    if looks_like_image(&lc) { return true; }
    JUNK_NEEDLES.iter().any(|n| lc.contains(n))
}

fn is_social_or_utility(host: &str) -> bool {
    SOCIAL_DOMAINS.iter().any(|s| host == *s || host.ends_with(&format!(".{s}")))
}

fn extract_emails_from_text(html: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let doc = Html::parse_document(html);
    if let Ok(sel) = Selector::parse(r#"a[href^="mailto:"]"#) {
        for a in doc.select(&sel) {
            if let Some(href) = a.value().attr("href") {
                let addr = href.trim_start_matches("mailto:").split(['?', '#']).next().unwrap_or("").trim();
                if !addr.is_empty() && EMAIL_RE.is_match(addr) && !is_junk(addr) {
                    out.insert(addr.to_string());
                }
            }
        }
    }
    for m in EMAIL_RE.find_iter(html) {
        let s = m.as_str();
        if !is_junk(s) { out.insert(s.to_string()); }
    }
    out
}

fn extract_emails_from_attrs(html: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let doc = Html::parse_document(html);
    if let Ok(sel) = Selector::parse("*") {
        for el in doc.select(&sel) {
            for (_name, val) in el.value().attrs() {
                if val.contains('@') {
                    for m in EMAIL_RE.find_iter(val) {
                        let s = m.as_str();
                        if !is_junk(s) { out.insert(s.to_string()); }
                    }
                }
            }
        }
    }
    out
}

fn extract_emails_from_scripts(html: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let doc = Html::parse_document(html);
    if let Ok(sel) = Selector::parse("script") {
        for script in doc.select(&sel) {
            let text = script.text().collect::<String>();
            if text.contains('@') {
                for m in EMAIL_RE.find_iter(&text) {
                    let s = m.as_str();
                    if !is_junk(s) { out.insert(s.to_string()); }
                }
            }
        }
    }
    out
}

fn scan_page(html: &str) -> HashSet<String> {
    let mut found = HashSet::new();
    found.extend(extract_emails_from_text(html));
    found.extend(extract_emails_from_attrs(html));
    found.extend(extract_emails_from_scripts(html));
    found
}

fn extract_contact_links(html: &str, base: &Url) -> Vec<Url> {
    let doc = Html::parse_document(html);
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let Ok(sel) = Selector::parse("a[href]") else { return out };
    for a in doc.select(&sel) {
        let Some(href) = a.value().attr("href") else { continue };
        if href.starts_with("mailto:") || href.starts_with("tel:") || href.starts_with('#') { continue }
        let text = a.text().collect::<String>().to_lowercase();
        let href_low = href.to_lowercase();
        let is_contact = CONTACT_KEYWORDS.iter().any(|k| href_low.contains(k) || text.contains(k));
        if !is_contact { continue }
        if let Ok(url) = base.join(href) {
            if !url.scheme().starts_with("http") { continue }
            if url.host_str() != base.host_str() { continue }
            let key = url.as_str().to_string();
            if seen.insert(key) { out.push(url); }
        }
        if out.len() >= 8 { break }
    }
    out
}

/// Extract external company-site links from a directory/listing page.
pub fn extract_external_company_links(html: &str, base: &Url) -> Vec<String> {
    let doc = Html::parse_document(html);
    let mut seen_domains = HashSet::new();
    let mut out = Vec::new();
    let Ok(sel) = Selector::parse("a[href]") else { return out };
    let base_host = base.host_str().unwrap_or("");

    for a in doc.select(&sel) {
        let Some(href) = a.value().attr("href") else { continue };
        if href.starts_with('#') || href.starts_with("mailto:") || href.starts_with("tel:") { continue }
        let Ok(url) = base.join(href) else { continue };
        if !url.scheme().starts_with("http") { continue }
        let Some(host) = url.host_str() else { continue };
        if host == base_host || host.ends_with(&format!(".{base_host}")) { continue }
        if is_social_or_utility(host) { continue }
        let domain = host.to_string();
        if seen_domains.insert(domain.clone()) {
            out.push(format!("{}://{}/", url.scheme(), host));
        }
        if out.len() >= 200 { break }
    }
    out
}

/// Fetch a URL and return external company links if it looks like a directory (≥3 distinct external domains).
pub async fn try_expand_directory(input: &str) -> Vec<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() { return vec![]; }
    let normalized = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let Ok(base) = Url::parse(&normalized) else { return vec![] };
    let Ok(client) = reqwest::Client::builder()
        .user_agent(concat!(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 ",
            "(KHTML, like Gecko) Chrome/124.0 Safari/537.36"
        ))
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build() else { return vec![] };
    let Ok(resp) = client.get(base.as_str()).send().await else { return vec![] };
    let Ok(html) = resp.text().await else { return vec![] };
    let links = extract_external_company_links(&html, &base);
    if links.len() >= 3 { links } else { vec![] }
}

/// Extract a human-readable company name from an HTML page or the URL itself.
pub fn extract_company_name(html: &str, base_url: &str) -> String {
    let doc = Html::parse_document(html);

    if let Ok(sel) = Selector::parse(r#"meta[property="og:site_name"]"#) {
        if let Some(el) = doc.select(&sel).next() {
            if let Some(content) = el.value().attr("content") {
                let name = content.trim().to_string();
                if !name.is_empty() && name.len() < 80 { return name; }
            }
        }
    }

    if let Ok(sel) = Selector::parse("title") {
        if let Some(el) = doc.select(&sel).next() {
            let text = el.text().collect::<String>();
            let name = text.split(['|', '-', '–', '—', ':']).next().unwrap_or("").trim().to_string();
            if !name.is_empty() && name.len() < 80 { return name; }
        }
    }

    if let Ok(url) = Url::parse(base_url) {
        if let Some(host) = url.host_str() {
            let domain = host.trim_start_matches("www.");
            let raw = domain.split('.').next().unwrap_or(domain);
            let mut chars = raw.chars();
            return match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            };
        }
    }
    "the company".to_string()
}

fn rank(addr: &str) -> i32 {
    let lc = addr.to_ascii_lowercase();
    let local = lc.split('@').next().unwrap_or("");
    let mut score = 0;
    for good in ["hr", "jobs", "careers", "career", "recruit", "hiring", "people", "talent", "kariyer", "ik", "staj", "intern"] {
        if local.contains(good) { score -= 100; }
    }
    for good in ["contact", "info", "hello", "office", "iletisim"] {
        if local.contains(good) { score -= 30; }
    }
    if local == "no-reply" || local == "noreply" || local.starts_with("donot") { score += 100; }
    score
}

/// Scrape a single company URL for contact emails. Returns (emails, company_name).
pub async fn scrape_emails_for_url(input: &str) -> Result<(Vec<String>, String), BotError> {
    let trimmed = input.trim();
    if trimmed.is_empty() { return Err(BotError::InvalidUrl(input.into())); }
    let normalized = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
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

    let mut found = HashSet::new();

    let resp = client.get(url.as_str()).send().await?;
    let final_url = resp.url().clone();
    let body = resp.text().await?;

    let company_name = extract_company_name(&body, final_url.as_str());
    found.extend(scan_page(&body));

    let contact_links = extract_contact_links(&body, &final_url);
    for link in contact_links {
        if let Ok(r) = client.get(link.as_str()).send().await {
            if let Ok(text) = r.text().await {
                found.extend(scan_page(&text));
            }
        }
    }

    let mut emails: Vec<String> = found.into_iter().collect();
    emails.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)));

    if emails.is_empty() { return Err(BotError::NoEmails(input.to_string())); }
    Ok((emails, company_name))
}
