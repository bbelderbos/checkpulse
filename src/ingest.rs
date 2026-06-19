use crate::AppState;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Deserialize)]
struct EventBody {
    path: String,
    referrer: Option<String>,
    name: Option<String>,
}

pub async fn ingest(State(state): State<AppState>, headers: HeaderMap, body: String) -> StatusCode {
    if header_value(&headers, "dnt").as_deref() == Some("1") {
        return StatusCode::NO_CONTENT;
    }

    if !origin_allowed(&headers, &state.config.allowed_origin) {
        return StatusCode::FORBIDDEN;
    }

    let user_agent = header_value(&headers, "user-agent").unwrap_or_default();
    if is_bot(&user_agent) {
        return StatusCode::NO_CONTENT;
    }

    let ip = client_ip(&headers);
    if !state.limiter.allow(&ip) {
        return StatusCode::TOO_MANY_REQUESTS;
    }

    let event: EventBody = match serde_json::from_str(&body) {
        Ok(e) => e,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    if !within_limits(
        &event.path,
        event.referrer.as_deref(),
        event.name.as_deref(),
    ) {
        return StatusCode::BAD_REQUEST;
    }

    let path = normalize_path(&event.path);
    if path.is_empty() {
        return StatusCode::BAD_REQUEST;
    }

    let name = event
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string);
    let (browser, device) = crate::ua::classify(&user_agent);

    let visitor_hash = state
        .salt
        .visitor_hash(&ip, &user_agent, &state.config.site_id);
    let referrer = referrer_host(event.referrer.as_deref(), &state.config.site_id);
    let country = state
        .geo
        .as_ref()
        .as_ref()
        .and_then(|g| ip.parse().ok().and_then(|addr| g.country(addr)));
    let ts = now_secs();

    let result = sqlx::query(
        "INSERT INTO events (site_id, ts, path, referrer, country, visitor_hash, name, browser, device)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&state.config.site_id)
    .bind(ts)
    .bind(&path)
    .bind(&referrer)
    .bind(&country)
    .bind(&visitor_hash)
    .bind(&name)
    .bind(browser)
    .bind(device)
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(e) => {
            tracing::error!("failed to insert event: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

pub async fn script(headers: HeaderMap) -> Response {
    let proto = header_value(&headers, "x-forwarded-proto").unwrap_or_else(|| "http".into());
    let host = header_value(&headers, "host").unwrap_or_else(|| "localhost".into());
    let endpoint = format!("{proto}://{host}/api/event");
    let js = build_snippet(&endpoint);

    Response::builder()
        .header(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(js))
        .unwrap()
        .into_response()
}

fn build_snippet(endpoint: &str) -> String {
    format!(
        r#"(function(){{
  if (navigator.doNotTrack === "1") return;
  var h = location.hostname;
  if (h === "localhost" || h === "127.0.0.1" || h === "[::1]" || h === "") return;
  var endpoint = "{endpoint}";
  var send = function(name){{
    try {{
      navigator.sendBeacon(endpoint, JSON.stringify({{
        path: location.pathname,
        referrer: name ? null : (document.referrer || null),
        name: name || null
      }}));
    }} catch (e) {{}}
  }};
  send();
  window.checkpulse = function(name){{ if (typeof name === "string" && name) send(name); }};
  var push = history.pushState;
  history.pushState = function(){{ push.apply(this, arguments); send(); }};
  window.addEventListener("popstate", function(){{ send(); }});
}})();
"#
    )
}

const MAX_TRACKED_IPS: usize = 10_000;

pub struct RateLimiter {
    limit: u32,
    window_secs: u64,
    state: Mutex<HashMap<String, (u64, u32)>>,
}

impl RateLimiter {
    pub fn new(limit: u32, window_secs: u64) -> Self {
        Self {
            limit,
            window_secs,
            state: Mutex::new(HashMap::new()),
        }
    }

    pub fn allow(&self, ip: &str) -> bool {
        let window = now_secs() as u64 / self.window_secs;
        let mut guard = self.state.lock().unwrap();
        if guard.len() > MAX_TRACKED_IPS {
            guard.retain(|_, (w, _)| *w == window);
            // Still over cap means a flood of distinct IPs in one window;
            // drop everything to keep memory bounded at the cost of resetting counters.
            if guard.len() > MAX_TRACKED_IPS {
                guard.clear();
            }
        }
        let entry = guard.entry(ip.to_string()).or_insert((window, 0));
        if entry.0 != window {
            *entry = (window, 0);
        }
        entry.1 += 1;
        entry.1 <= self.limit
    }
}

const BOT_UA_TOKENS: &[&str] = &[
    "bot",
    "spider",
    "crawl",
    "slurp",
    "headless",
    "scanner",
    "curl",
    "wget",
    "python-requests",
    "go-http-client",
    "phantomjs",
    "facebookexternalhit",
    "embedly",
];

fn is_bot(user_agent: &str) -> bool {
    let ua = user_agent.to_lowercase();
    // Every real browser sends a UA containing "Mozilla" via sendBeacon; anything
    // without it (empty, or a non-browser HTTP client) is automation.
    if !ua.contains("mozilla") {
        return true;
    }
    BOT_UA_TOKENS.iter().any(|token| ua.contains(token))
}

fn origin_allowed(headers: &HeaderMap, allowed: &str) -> bool {
    if allowed.is_empty() {
        return true; // enforcement disabled
    }
    if let Some(origin) = header_value(headers, "origin") {
        return origin == allowed;
    }
    // Some clients omit Origin; fall back to the Referer's scheme://host.
    if let Some(referer) = header_value(headers, "referer") {
        return referer_origin(&referer).as_deref() == Some(allowed);
    }
    false
}

fn referer_origin(referer: &str) -> Option<String> {
    let (scheme, rest) = referer.split_once("://")?;
    let host = rest.split('/').next().filter(|h| !h.is_empty())?;
    Some(format!("{scheme}://{host}"))
}

fn client_ip(headers: &HeaderMap) -> String {
    let candidates = [
        header_value(headers, "fly-client-ip"),
        header_value(headers, "x-forwarded-for")
            .and_then(|xff| xff.split(',').next().map(|s| s.trim().to_string())),
        header_value(headers, "x-real-ip"),
    ];
    candidates
        .into_iter()
        .flatten()
        .find_map(|v| v.parse::<IpAddr>().ok())
        .map(|ip| ip.to_string())
        .unwrap_or_default()
}

fn header_value(headers: &HeaderMap, key: &str) -> Option<String> {
    headers
        .get(key)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

const MAX_PATH_LEN: usize = 1024;
const MAX_REFERRER_LEN: usize = 2048;
const MAX_NAME_LEN: usize = 64;

fn within_limits(path: &str, referrer: Option<&str>, name: Option<&str>) -> bool {
    path.len() <= MAX_PATH_LEN
        && referrer.is_none_or(|r| r.len() <= MAX_REFERRER_LEN)
        && name.is_none_or(|n| !n.is_empty() && n.len() <= MAX_NAME_LEN)
}

fn normalize_path(raw: &str) -> String {
    let path = raw.split(['?', '#']).next().unwrap_or(raw).trim();
    if path.is_empty() {
        return String::new();
    }
    // Collapse a trailing slash (except root) so "/post/" and "/post" aggregate together.
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

fn referrer_host(raw: Option<&str>, site_id: &str) -> Option<String> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    let without_scheme = raw.split("://").nth(1).unwrap_or(raw);
    let host = without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .trim_start_matches("www.")
        .to_lowercase();
    if host.is_empty() || host == site_id || host == format!("www.{site_id}") {
        return None;
    }
    Some(host)
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_limits_rejects_oversized_fields() {
        assert!(within_limits(
            "/articles/rust",
            Some("https://news.ycombinator.com/item?id=1"),
            None
        ));
        assert!(within_limits("/", None, None));
        assert!(!within_limits(&"/".repeat(MAX_PATH_LEN + 1), None, None));
        assert!(!within_limits(
            "/",
            Some(&"a".repeat(MAX_REFERRER_LEN + 1)),
            None
        ));
    }

    #[test]
    fn within_limits_guards_event_name() {
        assert!(within_limits("/", None, Some("newsletter-signup")));
        assert!(!within_limits("/", None, Some("")));
        assert!(!within_limits(
            "/",
            None,
            Some(&"x".repeat(MAX_NAME_LEN + 1))
        ));
    }

    #[test]
    fn normalize_strips_query_and_trailing_slash() {
        assert_eq!(normalize_path("/articles/rust/?utm=x"), "/articles/rust");
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path(""), "");
    }

    #[test]
    fn referrer_reduces_to_host_and_drops_self() {
        assert_eq!(
            referrer_host(
                Some("https://news.ycombinator.com/item?id=1"),
                "belderbos.dev"
            ),
            Some("news.ycombinator.com".into())
        );
        assert_eq!(
            referrer_host(Some("https://belderbos.dev/x"), "belderbos.dev"),
            None
        );
        assert_eq!(
            referrer_host(Some("www.belderbos.dev"), "belderbos.dev"),
            None
        );
        assert_eq!(referrer_host(None, "belderbos.dev"), None);
    }

    #[test]
    fn is_bot_flags_crawlers_and_tools() {
        assert!(is_bot(
            "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)"
        ));
        assert!(is_bot("curl/8.4.0"));
        assert!(is_bot("HeadlessChrome/120.0.0.0"));
        assert!(is_bot("facebookexternalhit/1.1"));

        assert!(!is_bot(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36"
        ));
    }

    #[test]
    fn is_bot_requires_a_browser_user_agent() {
        // Empty or non-browser UAs never reach the JS beacon legitimately.
        assert!(is_bot(""));
        assert!(is_bot("python-requests/2.31.0"));
        assert!(is_bot("my-custom-scraper/1.0"));
        // A real browser UA always carries "Mozilla".
        assert!(!is_bot(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/126.0.0.0"
        ));
    }

    #[test]
    fn origin_check_blocks_mismatch_and_missing() {
        let allowed = "https://belderbos.dev";

        let mut good = HeaderMap::new();
        good.insert("origin", "https://belderbos.dev".parse().unwrap());
        assert!(origin_allowed(&good, allowed));

        let mut evil = HeaderMap::new();
        evil.insert("origin", "https://evil.com".parse().unwrap());
        assert!(!origin_allowed(&evil, allowed));

        let mut via_referer = HeaderMap::new();
        via_referer.insert(
            "referer",
            "https://belderbos.dev/articles/x".parse().unwrap(),
        );
        assert!(origin_allowed(&via_referer, allowed));

        assert!(!origin_allowed(&HeaderMap::new(), allowed));
        assert!(origin_allowed(&HeaderMap::new(), "")); // empty = disabled
    }

    #[test]
    fn limiter_blocks_after_limit() {
        let limiter = RateLimiter::new(2, 60);
        assert!(limiter.allow("1.1.1.1"));
        assert!(limiter.allow("1.1.1.1"));
        assert!(!limiter.allow("1.1.1.1"));
        assert!(limiter.allow("2.2.2.2"));
    }

    #[test]
    fn limiter_caps_tracked_ips() {
        let limiter = RateLimiter::new(1000, 60);
        for i in 0..(MAX_TRACKED_IPS + 100) {
            limiter.allow(&format!("10.0.{}.{}", i / 256, i % 256));
        }
        assert!(limiter.state.lock().unwrap().len() <= MAX_TRACKED_IPS);
    }

    #[test]
    fn snippet_embeds_endpoint_and_guards() {
        let js = build_snippet("https://checkpulse.fly.dev/api/event");
        assert!(js.contains(r#"var endpoint = "https://checkpulse.fly.dev/api/event";"#));
        assert!(js.contains("navigator.sendBeacon(endpoint"));
        assert!(js.contains(r#"navigator.doNotTrack === "1""#));
        assert!(js.contains("history.pushState"));
        assert!(js.contains("window.checkpulse"));
    }

    #[test]
    fn client_ip_validates_and_normalizes() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "not-an-ip, 1.2.3.4".parse().unwrap());
        assert_eq!(client_ip(&headers), "");

        let mut headers = HeaderMap::new();
        headers.insert("fly-client-ip", "garbage".parse().unwrap());
        headers.insert("x-real-ip", "1.2.3.4".parse().unwrap());
        assert_eq!(client_ip(&headers), "1.2.3.4");

        let mut headers = HeaderMap::new();
        headers.insert(
            "fly-client-ip",
            "2001:0db8:0000:0000:0000:0000:0000:0001".parse().unwrap(),
        );
        assert_eq!(client_ip(&headers), "2001:db8::1");

        assert_eq!(client_ip(&HeaderMap::new()), "");
    }
}
