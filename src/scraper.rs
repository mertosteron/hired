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

static FROMCHARCODE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"String\.fromCharCode\(\s*([0-9 ,]+)\s*\)").unwrap()
});

static B64_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"['"]([A-Za-z0-9+/]{12,}={0,2})['"]"#).unwrap()
});

static CSS_CONTENT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"content\s*:\s*['"]([^'"]+)['"]"#).unwrap()
});

const CONTACT_KEYWORDS: &[&str] = &[
    "contact", "kontakt", "about", "career", "careers", "jobs", "join",
    "hiring", "hr", "people", "team", "recruit", "talent", "apply",
    "iletisim", "iletişim", "hakkimizda", "hakkında", "kariyer",
    "bize-ulasin", "bize-ulaşın", "insan-kaynaklari", "staj", "intern",
    "başvuru", "basvuru", "reach", "connect", "support",
];

/// URL substrings worth a second-level dive on contact-style pages.
const DEEP_CRAWL_KEYWORDS: &[&str] = &[
    "contact", "iletisim", "iletişim", "reach", "connect", "support", "team",
];

const JUNK_NEEDLES: &[&str] = &[
    "example.com", "yourdomain", "yourcompany", "domain.com",
    "u003c", "u002",
];

const TRANSACTIONAL_LOCALS: &[&str] = &[
    "noreply", "no-reply", "donotreply", "do-not-reply",
    "bounce", "bounces", "mailer-daemon", "mailerdaemon", "postmaster",
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

fn is_transactional(addr: &str) -> bool {
    let lc = addr.to_ascii_lowercase();
    let local = lc.split('@').next().unwrap_or("");
    TRANSACTIONAL_LOCALS.iter().any(|t| local == *t || local.starts_with(&format!("{t}+")))
}

fn is_junk(addr: &str) -> bool {
    let lc = addr.to_ascii_lowercase();
    if looks_like_image(&lc) { return true; }
    if is_transactional(&lc) { return true; }
    JUNK_NEEDLES.iter().any(|n| lc.contains(n))
}

fn is_social_or_utility(host: &str) -> bool {
    SOCIAL_DOMAINS.iter().any(|s| host == *s || host.ends_with(&format!(".{s}")))
}

fn push_emails(out: &mut HashSet<String>, text: &str) {
    for m in EMAIL_RE.find_iter(text) {
        let s = m.as_str();
        if !is_junk(s) { out.insert(s.to_string()); }
    }
}

// ---------- Stages 1–5: original extraction ----------

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
    push_emails(&mut out, html);
    out
}

fn extract_emails_from_attrs(html: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let doc = Html::parse_document(html);
    if let Ok(sel) = Selector::parse("*") {
        for el in doc.select(&sel) {
            for (_name, val) in el.value().attrs() {
                if val.contains('@') {
                    push_emails(&mut out, val);
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
                push_emails(&mut out, &text);
            }
        }
    }
    out
}

// ---------- Stage 6: deobfuscation ----------

fn rot13(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='M' | 'a'..='m' => ((c as u8) + 13) as char,
            'N'..='Z' | 'n'..='z' => ((c as u8) - 13) as char,
            _ => c,
        })
        .collect()
}

fn deobfuscate(s: &str) -> String {
    let mut out = s.to_string();
    // Common "[at]", "(at)", "{at}", " AT ", "&#64;", "&commat;", fullwidth.
    let at_patterns = [
        " [at] ", "[at]", "[AT]", " [AT] ",
        " (at) ", "(at)", "(AT)", " (AT) ",
        " {at} ", "{at}", "{AT}",
        " at ", " AT ",
        "&#64;", "&commat;",
    ];
    for p in at_patterns { out = out.replace(p, "@"); }
    let dot_patterns = [
        " [dot] ", "[dot]", "[DOT]", " [DOT] ",
        " (dot) ", "(dot)", "(DOT)",
        " {dot} ", "{dot}", "{DOT}",
        " dot ", " DOT ",
        "&#46;",
    ];
    for p in dot_patterns { out = out.replace(p, "."); }
    out.replace('＠', "@").replace('．', ".").replace('・', ".")
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = s
        .bytes()
        .filter(|b| !matches!(b, b'=' | b'\n' | b'\r' | b' ' | b'\t'))
        .collect();
    if bytes.len() < 4 { return None; }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 { return None; }
        let v0 = val(chunk[0])?;
        let v1 = val(chunk[1])?;
        out.push((v0 << 2) | (v1 >> 4));
        if chunk.len() > 2 {
            let v2 = val(chunk[2])?;
            out.push(((v1 & 0xf) << 4) | (v2 >> 2));
            if chunk.len() > 3 {
                let v3 = val(chunk[3])?;
                out.push(((v2 & 0x3) << 6) | v3);
            }
        }
    }
    Some(out)
}

fn extract_emails_from_obfuscation(html: &str) -> HashSet<String> {
    let mut out = HashSet::new();

    // Textual obfuscation: [at], (at), unicode lookalikes, HTML entities.
    let deob = deobfuscate(html);
    push_emails(&mut out, &deob);

    // ROT13.
    let rotated = rot13(html);
    push_emails(&mut out, &rotated);

    // Reversed strings (some scripts embed reversed mailto, e.g. "moc.foo@bar").
    let reversed: String = html.chars().rev().collect();
    push_emails(&mut out, &reversed);

    // String.fromCharCode(72,105,...) inside scripts.
    for cap in FROMCHARCODE_RE.captures_iter(html) {
        if let Some(nums) = cap.get(1) {
            let decoded: String = nums
                .as_str()
                .split(',')
                .filter_map(|n| n.trim().parse::<u32>().ok())
                .filter_map(char::from_u32)
                .collect();
            push_emails(&mut out, &decoded);
        }
    }

    // base64-encoded mailto blobs (atob('...')) and similar.
    for cap in B64_RE.captures_iter(html) {
        if let Some(b64) = cap.get(1) {
            if let Some(decoded) = base64_decode(b64.as_str()) {
                if let Ok(s) = std::str::from_utf8(&decoded) {
                    if s.contains('@') {
                        push_emails(&mut out, s);
                    }
                }
            }
        }
    }

    out
}

// ---------- Stage 7: data-* attributes ----------

fn extract_emails_from_data_attrs(html: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let doc = Html::parse_document(html);
    let direct = ["data-email", "data-contact", "data-mail", "data-address"];
    let Ok(sel) = Selector::parse("*") else { return out };

    for el in doc.select(&sel) {
        let mut user: Option<String> = None;
        let mut domain: Option<String> = None;
        for (name, val) in el.value().attrs() {
            let lname = name.to_ascii_lowercase();
            if direct.contains(&lname.as_str()) {
                let v = val.trim();
                push_emails(&mut out, v);
                let deob = deobfuscate(v);
                push_emails(&mut out, &deob);
            } else if lname == "data-user" || lname == "data-local" {
                user = Some(val.trim().to_string());
            } else if lname == "data-domain" || lname == "data-host" {
                domain = Some(val.trim().to_string());
            }
        }
        if let (Some(u), Some(d)) = (user, domain) {
            let candidate = format!("{u}@{d}");
            if EMAIL_RE.is_match(&candidate) && !is_junk(&candidate) {
                out.insert(candidate);
            }
        }
    }
    out
}

// ---------- Stage 8: CSS content injection ----------

fn scan_css_text(text: &str, out: &mut HashSet<String>) {
    for cap in CSS_CONTENT_RE.captures_iter(text) {
        if let Some(c) = cap.get(1) {
            push_emails(out, c.as_str());
            let deob = deobfuscate(c.as_str());
            push_emails(out, &deob);
        }
    }
}

fn extract_emails_from_css(html: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let doc = Html::parse_document(html);

    if let Ok(sel) = Selector::parse("style") {
        for el in doc.select(&sel) {
            let text = el.text().collect::<String>();
            scan_css_text(&text, &mut out);
        }
    }
    if let Ok(sel) = Selector::parse("[style]") {
        for el in doc.select(&sel) {
            if let Some(s) = el.value().attr("style") {
                scan_css_text(s, &mut out);
            }
        }
    }
    out
}

// ---------- Page scan: all stages combined ----------

fn scan_page(html: &str) -> HashSet<String> {
    let mut found = HashSet::new();
    found.extend(extract_emails_from_text(html));        // stage 1+2
    found.extend(extract_emails_from_attrs(html));        // stage 3
    found.extend(extract_emails_from_scripts(html));      // stage 4
    found.extend(extract_emails_from_obfuscation(html));  // stage 6
    found.extend(extract_emails_from_data_attrs(html));   // stage 7
    found.extend(extract_emails_from_css(html));          // stage 8
    found
}

// ---------- Contact-page link extraction ----------

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
    let mut visited: HashSet<String> = HashSet::new();

    let resp = client.get(url.as_str()).send().await?;
    let final_url = resp.url().clone();
    visited.insert(final_url.as_str().to_string());
    let body = resp.text().await?;

    let company_name = extract_company_name(&body, final_url.as_str());
    found.extend(scan_page(&body));

    let contact_links = extract_contact_links(&body, &final_url);
    let mut deep_budget: usize = 6;

    for link in contact_links {
        if !visited.insert(link.as_str().to_string()) { continue }
        let Ok(r) = client.get(link.as_str()).send().await else { continue };
        let Ok(text) = r.text().await else { continue };
        found.extend(scan_page(&text));

        // Stage 9: drill one level deeper on contact-style URLs.
        let link_low = link.as_str().to_ascii_lowercase();
        if DEEP_CRAWL_KEYWORDS.iter().any(|k| link_low.contains(k)) && deep_budget > 0 {
            let sublinks = extract_contact_links(&text, &link);
            for sub in sublinks {
                if deep_budget == 0 { break }
                if !visited.insert(sub.as_str().to_string()) { continue }
                if let Ok(r2) = client.get(sub.as_str()).send().await {
                    if let Ok(t2) = r2.text().await {
                        found.extend(scan_page(&t2));
                    }
                }
                deep_budget -= 1;
            }
        }
    }

    let mut emails: Vec<String> = found.into_iter().collect();
    emails.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)));

    if emails.is_empty() { return Err(BotError::NoEmails(input.to_string())); }
    Ok((emails, company_name))
}
