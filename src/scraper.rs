use crate::error::BotError;
use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use url::Url;

static EMAIL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,24}").unwrap()
});

static SITEMAP_LOC_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)<loc[^>]*>\s*([^<\s]+)\s*</loc>").unwrap()
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

/// Paths to try directly on every domain, even if nothing in the nav links to them.
const WELL_KNOWN_CONTACT_PATHS: &[&str] = &[
    "/contact", "/contacts", "/contact-us", "/contactus",
    "/iletisim", "/iletişim", "/iletisim-bilgileri", "/bize-ulasin",
    "/about", "/about-us", "/about/contact",
    "/hakkimizda", "/hakkinda", "/hakkımızda",
    "/jobs", "/careers", "/career", "/career/contact",
    "/work-with-us", "/join-us", "/join",
    "/hiring", "/team", "/people", "/ekibimiz", "/ekip",
    "/kariyer", "/kariyer/iletisim", "/insan-kaynaklari", "/ik",
    "/impressum", "/imprint", "/legal",
    "/staj", "/intern", "/internship",
];

const JUNK_NEEDLES: &[&str] = &[
    "example.com", "yourdomain", "yourcompany", "domain.com",
    "u003c", "u003e", "u002", "x3c", "x3e",
];

/// Local-parts that we drop entirely — never useful for a job application.
const TRANSACTIONAL_LOCALS: &[&str] = &[
    "noreply", "no-reply", "donotreply", "do-not-reply",
    "bounce", "bounces", "mailer-daemon", "mailerdaemon", "postmaster",
    "unsubscribe", "support", "help",
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

    // ROT13 and reversed strings: high-recall but high-false-positive (any
    // long alphabetic run can decode to a syntactically valid email). Only
    // keep results whose host is also literally present in the source HTML,
    // which proves the decode was a real obfuscated address.
    let mut conservative: HashSet<String> = HashSet::new();
    let rotated = rot13(html);
    push_emails(&mut conservative, &rotated);
    let reversed: String = html.chars().rev().collect();
    push_emails(&mut conservative, &reversed);
    let html_low = html.to_ascii_lowercase();
    for addr in conservative {
        let host = addr.split('@').nth(1).unwrap_or("").to_ascii_lowercase();
        if !host.is_empty() && html_low.contains(&host) {
            out.insert(addr);
        }
    }

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

// ---------- Stage 11: Cloudflare email obfuscation ----------

/// Decode a Cloudflare `data-cfemail` / `email-protection#<hex>` payload.
///
/// Algorithm: the first byte is the XOR key; every subsequent byte is the
/// next character of the email after XOR-ing it with the key.
pub fn decode_cfemail(hex: &str) -> Option<String> {
    let hex = hex.trim();
    if hex.len() < 4 || hex.len() % 2 != 0 { return None; }
    let bytes: Option<Vec<u8>> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect();
    let bytes = bytes?;
    let key = bytes[0];
    let mut out = String::with_capacity(bytes.len() - 1);
    for &b in &bytes[1..] {
        let c = b ^ key;
        if !c.is_ascii() { return None; }
        out.push(c as char);
    }
    if EMAIL_RE.is_match(&out) { Some(out) } else { None }
}

fn extract_emails_from_cloudflare(html: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let doc = Html::parse_document(html);

    // (a) data-cfemail attribute on any element (the visible link form).
    if let Ok(sel) = Selector::parse("[data-cfemail]") {
        for el in doc.select(&sel) {
            if let Some(hex) = el.value().attr("data-cfemail") {
                if let Some(addr) = decode_cfemail(hex) {
                    if !is_junk(&addr) { out.insert(addr); }
                }
            }
        }
    }

    // (b) /cdn-cgi/l/email-protection#<hex> in any href (fallback link form).
    if let Ok(sel) = Selector::parse(r#"a[href*="/cdn-cgi/l/email-protection"]"#) {
        for a in doc.select(&sel) {
            if let Some(href) = a.value().attr("href") {
                if let Some(hex) = href.split('#').nth(1) {
                    if let Some(addr) = decode_cfemail(hex) {
                        if !is_junk(&addr) { out.insert(addr); }
                    }
                }
            }
        }
    }

    out
}

// ---------- Page scan: all stages combined ----------

/// Provenance label for a single email finding. Cheap to clone — &'static str.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FindingMethod {
    Mailto,
    PlainText,
    Attribute,
    Script,
    Obfuscated,
    DataAttr,
    Css,
    Cloudflare,
}

impl FindingMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            FindingMethod::Mailto => "mailto:",
            FindingMethod::PlainText => "plain text",
            FindingMethod::Attribute => "attribute",
            FindingMethod::Script => "script",
            FindingMethod::Obfuscated => "obfuscated",
            FindingMethod::DataAttr => "data-*",
            FindingMethod::Css => "css content",
            FindingMethod::Cloudflare => "cloudflare cfemail",
        }
    }
}

/// One discovered email with provenance — used by the verify harness.
#[derive(Debug, Clone)]
pub struct EmailFinding {
    pub email: String,
    pub source_url: String,
    pub method: FindingMethod,
    pub category: EmailCategory,
}

/// Coarse category, ordered to mirror application-priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailCategory {
    /// kariyer/jobs/hr/recruitment/hiring/ik
    Careers,
    /// info/iletisim/contact/hello/office
    General,
    /// everything else legitimate (personal addresses, departments, etc.)
    Other,
}

impl EmailCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            EmailCategory::Careers => "careers",
            EmailCategory::General => "general",
            EmailCategory::Other => "other",
        }
    }

    pub fn from_email(addr: &str) -> Self {
        let lc = addr.to_ascii_lowercase();
        let local = lc.split('@').next().unwrap_or("");
        for k in ["hr", "jobs", "career", "careers", "recruit", "hiring", "talent",
                  "people", "kariyer", "ik", "insankaynak", "staj", "intern"] {
            if local.contains(k) { return EmailCategory::Careers; }
        }
        for k in ["info", "contact", "iletisim", "hello", "office", "merhaba"] {
            if local.contains(k) { return EmailCategory::General; }
        }
        EmailCategory::Other
    }
}

/// Run every stage on `html` and return a (method, set-of-emails) map.
fn scan_page_provenanced(html: &str) -> HashMap<FindingMethod, HashSet<String>> {
    let mut out: HashMap<FindingMethod, HashSet<String>> = HashMap::new();

    // Stage 1 (mailto:) is folded into extract_emails_from_text already,
    // but we re-scan mailto links separately to label them precisely.
    let doc = Html::parse_document(html);
    if let Ok(sel) = Selector::parse(r#"a[href^="mailto:"]"#) {
        let entry = out.entry(FindingMethod::Mailto).or_default();
        for a in doc.select(&sel) {
            if let Some(href) = a.value().attr("href") {
                let addr = href.trim_start_matches("mailto:")
                    .split(['?', '#'])
                    .next().unwrap_or("").trim();
                if !addr.is_empty() && EMAIL_RE.is_match(addr) && !is_junk(addr) {
                    entry.insert(addr.to_string());
                }
            }
        }
    }

    // For plain text we re-run the body scan but exclude anything already found
    // as a mailto so the label stays accurate.
    let mailto_set = out.get(&FindingMethod::Mailto).cloned().unwrap_or_default();
    let mut plain = HashSet::new();
    push_emails(&mut plain, html);
    for e in &mailto_set { plain.remove(e); }
    if !plain.is_empty() { out.insert(FindingMethod::PlainText, plain); }

    let attrs = extract_emails_from_attrs(html);
    if !attrs.is_empty() { out.insert(FindingMethod::Attribute, attrs); }

    let scripts = extract_emails_from_scripts(html);
    if !scripts.is_empty() { out.insert(FindingMethod::Script, scripts); }

    let obf = extract_emails_from_obfuscation(html);
    if !obf.is_empty() { out.insert(FindingMethod::Obfuscated, obf); }

    let data = extract_emails_from_data_attrs(html);
    if !data.is_empty() { out.insert(FindingMethod::DataAttr, data); }

    let css = extract_emails_from_css(html);
    if !css.is_empty() { out.insert(FindingMethod::Css, css); }

    let cf = extract_emails_from_cloudflare(html);
    if !cf.is_empty() { out.insert(FindingMethod::Cloudflare, cf); }

    out
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

/// Lower score sorts earlier. Penalize transactional/off-domain, reward
/// careers/general buckets.
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

/// Score an email *in the context of* the base URL it was scraped from.
/// Adds an off-domain penalty so addresses whose host doesn't match the site
/// are still kept but sorted lower than addresses on the company's own domain.
fn rank_with_context(addr: &str, base_host: &str) -> i32 {
    let mut score = rank(addr);
    if is_off_domain(addr, base_host) { score += 25; }
    score
}

/// True when the email's host doesn't appear to belong to the company's
/// own domain. Two-label suffix match handles `foo.com` and `foo.com.tr`.
fn is_off_domain(addr: &str, base_host: &str) -> bool {
    let lc = addr.to_ascii_lowercase();
    let Some(email_host) = lc.split('@').nth(1) else { return false };
    let base = base_host.trim_start_matches("www.").to_ascii_lowercase();
    if base.is_empty() { return false; }
    if email_host == base || email_host.ends_with(&format!(".{base}")) { return false; }
    // Also accept "subdomain.x" → "x" registrable-domain match.
    let last_two = |h: &str| -> String {
        let parts: Vec<&str> = h.split('.').collect();
        if parts.len() >= 2 {
            parts[parts.len() - 2..].join(".")
        } else {
            h.to_string()
        }
    };
    last_two(email_host) != last_two(&base)
}

// ---------- Sitemap discovery ----------

/// Fetch `/sitemap.xml` and return contact-flavored same-host URLs. Capped
/// so a 5,000-entry sitemap can't blow up the crawl budget.
async fn discover_sitemap_pages(
    client: &reqwest::Client,
    base: &Url,
) -> Vec<Url> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let Ok(sm_url) = base.join("/sitemap.xml") else { return out };
    let Ok(resp) = client.get(sm_url.as_str()).send().await else { return out };
    if !resp.status().is_success() { return out }
    let Ok(body) = resp.text().await else { return out };

    let base_host = base.host_str().unwrap_or("");
    for cap in SITEMAP_LOC_RE.captures_iter(&body) {
        let Some(loc) = cap.get(1) else { continue };
        let loc = loc.as_str().trim();
        let Ok(u) = Url::parse(loc) else { continue };
        if u.host_str() != Some(base_host) { continue }
        let path_low = u.path().to_ascii_lowercase();
        let interesting = CONTACT_KEYWORDS.iter().any(|k| path_low.contains(k));
        if !interesting { continue }
        let key = u.as_str().to_string();
        if seen.insert(key) { out.push(u); }
        if out.len() >= 20 { break }
    }
    out
}

/// Result of a single-site scrape with full provenance — used by the verify
/// harness and by the bot's compatibility shim.
#[derive(Debug, Clone)]
pub struct ScrapeReport {
    pub company_name: String,
    pub final_url: String,
    pub findings: Vec<EmailFinding>,
}

impl ScrapeReport {
    /// Best email — first by application priority (careers > general > other),
    /// then by on-domain-ness, then alphabetically.
    pub fn primary(&self) -> Option<&EmailFinding> { self.findings.first() }

    /// Just the email strings, sorted in priority order.
    pub fn emails(&self) -> Vec<String> {
        self.findings.iter().map(|f| f.email.clone()).collect()
    }
}

fn merge_findings(
    out: &mut HashMap<String, EmailFinding>,
    page_url: &str,
    base_host: &str,
    page_results: HashMap<FindingMethod, HashSet<String>>,
) {
    // Method preference: mailto beats everything (most authoritative),
    // then cloudflare, then plain text, then everything else.
    fn method_rank(m: FindingMethod) -> u8 {
        match m {
            FindingMethod::Mailto => 0,
            FindingMethod::Cloudflare => 1,
            FindingMethod::PlainText => 2,
            FindingMethod::DataAttr => 3,
            FindingMethod::Attribute => 4,
            FindingMethod::Obfuscated => 5,
            FindingMethod::Script => 6,
            FindingMethod::Css => 7,
        }
    }
    for (method, set) in page_results {
        for addr in set {
            let key = addr.to_ascii_lowercase();
            let new_score = method_rank(method);
            let category = EmailCategory::from_email(&addr);
            let _ = base_host; // captured for symmetry; off-domain logic happens at rank time
            out.entry(key)
                .and_modify(|existing| {
                    if method_rank(existing.method) > new_score {
                        existing.method = method;
                        existing.source_url = page_url.to_string();
                    }
                })
                .or_insert(EmailFinding {
                    email: addr,
                    source_url: page_url.to_string(),
                    method,
                    category,
                });
        }
    }
}

fn build_client() -> Result<reqwest::Client, BotError> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 ",
            "(KHTML, like Gecko) Chrome/124.0 Safari/537.36 HiredBot/0.1"
        ))
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?)
}

/// Scrape a single company URL and return everything we found with provenance.
///
/// Crawl plan:
/// 1. Fetch the home page, scan with every stage.
/// 2. Follow contact-keyword links found in the nav/footer (one level deep,
///    optionally a second level on contact-style URLs).
/// 3. Pull contact-keyword `<loc>` entries out of `/sitemap.xml` and scan those.
/// 4. Probe `WELL_KNOWN_CONTACT_PATHS` directly even if nothing linked to them.
pub async fn scrape_with_provenance(input: &str) -> Result<ScrapeReport, BotError> {
    let trimmed = input.trim();
    if trimmed.is_empty() { return Err(BotError::InvalidUrl(input.into())); }
    let normalized = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let url = Url::parse(&normalized).map_err(|_| BotError::InvalidUrl(input.into()))?;

    let client = build_client()?;
    log::info!("scrape: starting {}", url.as_str());

    let mut findings: HashMap<String, EmailFinding> = HashMap::new();
    let mut visited: HashSet<String> = HashSet::new();

    // 1) Home page.
    let resp = client.get(url.as_str()).send().await?;
    let final_url = resp.url().clone();
    let base_host = final_url.host_str().unwrap_or("").to_string();
    visited.insert(final_url.as_str().to_string());
    let body = resp.text().await?;
    let company_name = extract_company_name(&body, final_url.as_str());
    merge_findings(&mut findings, final_url.as_str(), &base_host, scan_page_provenanced(&body));

    // 2) Contact-keyword links from the home page.
    let contact_links = extract_contact_links(&body, &final_url);
    let mut deep_budget: usize = 6;
    for link in contact_links {
        if !visited.insert(link.as_str().to_string()) { continue }
        let Ok(r) = client.get(link.as_str()).send().await else { continue };
        let Ok(text) = r.text().await else { continue };
        merge_findings(&mut findings, link.as_str(), &base_host, scan_page_provenanced(&text));

        let link_low = link.as_str().to_ascii_lowercase();
        if DEEP_CRAWL_KEYWORDS.iter().any(|k| link_low.contains(k)) && deep_budget > 0 {
            for sub in extract_contact_links(&text, &link) {
                if deep_budget == 0 { break }
                if !visited.insert(sub.as_str().to_string()) { continue }
                if let Ok(r2) = client.get(sub.as_str()).send().await {
                    if let Ok(t2) = r2.text().await {
                        merge_findings(&mut findings, sub.as_str(), &base_host, scan_page_provenanced(&t2));
                    }
                }
                deep_budget -= 1;
            }
        }
    }

    // 3) Sitemap-driven discovery.
    for sm_url in discover_sitemap_pages(&client, &final_url).await {
        if !visited.insert(sm_url.as_str().to_string()) { continue }
        let Ok(r) = client.get(sm_url.as_str()).send().await else { continue };
        if !r.status().is_success() { continue }
        let Ok(text) = r.text().await else { continue };
        merge_findings(&mut findings, sm_url.as_str(), &base_host, scan_page_provenanced(&text));
    }

    // 4) Well-known contact paths.
    for path in WELL_KNOWN_CONTACT_PATHS {
        let Ok(candidate) = final_url.join(path) else { continue };
        if !visited.insert(candidate.as_str().to_string()) { continue }
        let Ok(r) = client.get(candidate.as_str()).send().await else { continue };
        if !r.status().is_success() { continue }
        let Ok(text) = r.text().await else { continue };
        merge_findings(&mut findings, candidate.as_str(), &base_host, scan_page_provenanced(&text));
    }

    let mut findings: Vec<EmailFinding> = findings.into_values().collect();
    findings.sort_by(|a, b| {
        rank_with_context(&a.email, &base_host)
            .cmp(&rank_with_context(&b.email, &base_host))
            .then_with(|| a.email.cmp(&b.email))
    });

    if findings.is_empty() {
        log::info!("scrape: {} found 0 emails", final_url);
        return Err(BotError::NoEmails(input.to_string()));
    }
    log::info!("scrape: {} found {} email(s)", final_url, findings.len());

    Ok(ScrapeReport {
        company_name,
        final_url: final_url.to_string(),
        findings,
    })
}

/// Scrape a single company URL for contact emails. Returns (emails, company_name).
///
/// Compatibility shim around [`scrape_with_provenance`] for the bot's send pipeline,
/// which only needs the email strings in priority order.
pub async fn scrape_emails_for_url(input: &str) -> Result<(Vec<String>, String), BotError> {
    let report = scrape_with_provenance(input).await?;
    Ok((report.emails(), report.company_name))
}
