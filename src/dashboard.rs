use crate::AppState;
use askama::Template;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Deserialize;
use sqlx::SqlitePool;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(sqlx::FromRow)]
pub struct Count {
    pub label: String,
    pub count: i64,
}

#[derive(Deserialize)]
pub struct DashboardQuery {
    period: Option<String>,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    period: String,
    site_id: String,
    total_views: i64,
    unique_visitors: i64,
    chart_labels: String,
    chart_values: String,
    top_pages: Vec<Count>,
    top_referrers: Vec<Count>,
    top_events: Vec<Count>,
    top_browsers: Vec<Count>,
    top_devices: Vec<Count>,
}

struct Period {
    key: &'static str,
    cutoff: i64,
    series_sql: &'static str,
}

const SERIES_HOURLY: &str = "SELECT strftime('%H:00', ts, 'unixepoch') AS label, COUNT(*) AS count
     FROM events WHERE site_id = ? AND ts >= ? AND name IS NULL GROUP BY label ORDER BY label";
const SERIES_DAILY: &str = "SELECT strftime('%m-%d', ts, 'unixepoch') AS label, COUNT(*) AS count
     FROM events WHERE site_id = ? AND ts >= ? AND name IS NULL GROUP BY label ORDER BY label";

fn resolve_period(raw: Option<&str>) -> Period {
    let now = now_secs();
    match raw {
        Some("today") => Period {
            key: "today",
            cutoff: (now / 86_400) * 86_400,
            series_sql: SERIES_HOURLY,
        },
        Some("30d") => Period {
            key: "30d",
            cutoff: now - 30 * 86_400,
            series_sql: SERIES_DAILY,
        },
        _ => Period {
            key: "7d",
            cutoff: now - 7 * 86_400,
            series_sql: SERIES_DAILY,
        },
    }
}

pub async fn dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DashboardQuery>,
) -> Response {
    if !authorized(
        &headers,
        &state.config.dashboard_user,
        &state.config.dashboard_password,
    ) {
        return unauthorized();
    }

    let period = resolve_period(query.period.as_deref());
    let site = &state.config.site_id;

    let total_views = scalar_count(
        &state.pool,
        "SELECT COUNT(*) FROM events WHERE site_id = ? AND ts >= ? AND name IS NULL",
        site,
        period.cutoff,
    )
    .await;
    let unique_visitors = scalar_count(
        &state.pool,
        "SELECT COUNT(DISTINCT visitor_hash) FROM events WHERE site_id = ? AND ts >= ? AND name IS NULL",
        site,
        period.cutoff,
    )
    .await;

    let series = grouped(&state.pool, period.series_sql, site, period.cutoff).await;
    let top_pages = grouped(
        &state.pool,
        "SELECT path AS label, COUNT(*) AS count
         FROM events WHERE site_id = ? AND ts >= ? AND name IS NULL
         GROUP BY path ORDER BY count DESC LIMIT 15",
        site,
        period.cutoff,
    )
    .await;
    let top_referrers = grouped(
        &state.pool,
        "SELECT referrer AS label, COUNT(*) AS count
         FROM events WHERE site_id = ? AND ts >= ? AND name IS NULL AND referrer IS NOT NULL
         GROUP BY referrer ORDER BY count DESC LIMIT 15",
        site,
        period.cutoff,
    )
    .await;
    let top_browsers = grouped(
        &state.pool,
        "SELECT browser AS label, COUNT(*) AS count
         FROM events WHERE site_id = ? AND ts >= ? AND name IS NULL AND browser IS NOT NULL
         GROUP BY browser ORDER BY count DESC LIMIT 15",
        site,
        period.cutoff,
    )
    .await;
    let top_devices = grouped(
        &state.pool,
        "SELECT device AS label, COUNT(*) AS count
         FROM events WHERE site_id = ? AND ts >= ? AND name IS NULL AND device IS NOT NULL
         GROUP BY device ORDER BY count DESC LIMIT 15",
        site,
        period.cutoff,
    )
    .await;
    let top_events = grouped(
        &state.pool,
        "SELECT name AS label, COUNT(*) AS count
         FROM events WHERE site_id = ? AND ts >= ? AND name IS NOT NULL
         GROUP BY name ORDER BY count DESC LIMIT 15",
        site,
        period.cutoff,
    )
    .await;

    let chart_labels = json_array(series.iter().map(|c| format!("\"{}\"", c.label)));
    let chart_values = json_array(series.iter().map(|c| c.count.to_string()));

    let page = DashboardTemplate {
        period: period.key.to_string(),
        site_id: site.clone(),
        total_views,
        unique_visitors,
        chart_labels,
        chart_values,
        top_pages,
        top_referrers,
        top_events,
        top_browsers,
        top_devices,
    };

    match page.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("template render failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn scalar_count(pool: &SqlitePool, sql: &'static str, site: &str, cutoff: i64) -> i64 {
    sqlx::query_scalar(sql)
        .bind(site)
        .bind(cutoff)
        .fetch_one(pool)
        .await
        .unwrap_or(0)
}

async fn grouped(pool: &SqlitePool, sql: &'static str, site: &str, cutoff: i64) -> Vec<Count> {
    sqlx::query_as::<_, Count>(sql)
        .bind(site)
        .bind(cutoff)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
}

fn json_array(items: impl Iterator<Item = String>) -> String {
    let inner = items.collect::<Vec<_>>().join(",");
    format!("[{inner}]")
}

fn authorized(headers: &HeaderMap, expected_user: &str, expected_pass: &str) -> bool {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let Some(encoded) = value.strip_prefix("Basic ") else {
        return false;
    };
    let Ok(decoded) = STANDARD.decode(encoded.trim()) else {
        return false;
    };
    let Ok(creds) = String::from_utf8(decoded) else {
        return false;
    };
    let Some((user, pass)) = creds.split_once(':') else {
        return false;
    };
    user == expected_user && pass == expected_pass
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"checkpulse\"")],
        "Unauthorized",
    )
        .into_response()
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

    fn auth_header(user: &str, pass: &str) -> HeaderMap {
        let token = STANDARD.encode(format!("{user}:{pass}"));
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Basic {token}").parse().unwrap(),
        );
        headers
    }

    #[test]
    fn resolve_period_selects_grain_and_cutoff() {
        let today = resolve_period(Some("today"));
        assert_eq!(today.key, "today");
        assert_eq!(today.series_sql, SERIES_HOURLY);
        assert_eq!(today.cutoff % 86_400, 0); // aligned to midnight UTC

        let month = resolve_period(Some("30d"));
        assert_eq!(month.key, "30d");
        assert_eq!(month.series_sql, SERIES_DAILY);

        // Unknown and missing both fall back to 7d/daily.
        assert_eq!(resolve_period(None).key, "7d");
        assert_eq!(resolve_period(Some("nonsense")).key, "7d");
        assert_eq!(resolve_period(Some("nonsense")).series_sql, SERIES_DAILY);

        // Wider windows reach further back.
        assert!(month.cutoff < resolve_period(None).cutoff);
        assert!(resolve_period(None).cutoff < today.cutoff);
    }

    #[test]
    fn json_array_wraps_and_joins() {
        assert_eq!(json_array(std::iter::empty()), "[]");
        assert_eq!(
            json_array(["1".to_string(), "2".to_string()].into_iter()),
            "[1,2]"
        );
        assert_eq!(
            json_array([r#""a""#.to_string(), r#""b""#.to_string()].into_iter()),
            r#"["a","b"]"#
        );
    }

    #[test]
    fn authorized_accepts_only_exact_credentials() {
        assert!(authorized(
            &auth_header("admin", "secret"),
            "admin",
            "secret"
        ));
        assert!(!authorized(
            &auth_header("admin", "wrong"),
            "admin",
            "secret"
        ));
        assert!(!authorized(
            &auth_header("eve", "secret"),
            "admin",
            "secret"
        ));
    }

    #[test]
    fn authorized_rejects_malformed_headers() {
        // Missing header.
        assert!(!authorized(&HeaderMap::new(), "admin", "secret"));

        // Wrong scheme.
        let mut bearer = HeaderMap::new();
        bearer.insert(header::AUTHORIZATION, "Bearer xyz".parse().unwrap());
        assert!(!authorized(&bearer, "admin", "secret"));

        // Not valid base64.
        let mut bad_b64 = HeaderMap::new();
        bad_b64.insert(header::AUTHORIZATION, "Basic !!!!".parse().unwrap());
        assert!(!authorized(&bad_b64, "admin", "secret"));

        // Decodes but has no colon separator.
        let mut no_colon = HeaderMap::new();
        let token = STANDARD.encode("adminsecret");
        no_colon.insert(
            header::AUTHORIZATION,
            format!("Basic {token}").parse().unwrap(),
        );
        assert!(!authorized(&no_colon, "admin", "secret"));
    }
}
