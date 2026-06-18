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
    top_countries: Vec<Count>,
}

struct Period {
    key: &'static str,
    cutoff: i64,
    series_sql: &'static str,
}

const SERIES_HOURLY: &str = "SELECT strftime('%H:00', ts, 'unixepoch') AS label, COUNT(*) AS count
     FROM events WHERE site_id = ? AND ts >= ? GROUP BY label ORDER BY label";
const SERIES_DAILY: &str = "SELECT strftime('%m-%d', ts, 'unixepoch') AS label, COUNT(*) AS count
     FROM events WHERE site_id = ? AND ts >= ? GROUP BY label ORDER BY label";

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
    if !authorized(&headers, &state) {
        return unauthorized();
    }

    let period = resolve_period(query.period.as_deref());
    let site = &state.config.site_id;

    let total_views = scalar_count(
        &state.pool,
        "SELECT COUNT(*) FROM events WHERE site_id = ? AND ts >= ?",
        site,
        period.cutoff,
    )
    .await;
    let unique_visitors = scalar_count(
        &state.pool,
        "SELECT COUNT(DISTINCT visitor_hash) FROM events WHERE site_id = ? AND ts >= ?",
        site,
        period.cutoff,
    )
    .await;

    let series = grouped(&state.pool, period.series_sql, site, period.cutoff).await;
    let top_pages = grouped(
        &state.pool,
        "SELECT path AS label, COUNT(*) AS count
         FROM events WHERE site_id = ? AND ts >= ?
         GROUP BY path ORDER BY count DESC LIMIT 15",
        site,
        period.cutoff,
    )
    .await;
    let top_referrers = grouped(
        &state.pool,
        "SELECT referrer AS label, COUNT(*) AS count
         FROM events WHERE site_id = ? AND ts >= ? AND referrer IS NOT NULL
         GROUP BY referrer ORDER BY count DESC LIMIT 15",
        site,
        period.cutoff,
    )
    .await;
    let top_countries = grouped(
        &state.pool,
        "SELECT country AS label, COUNT(*) AS count
         FROM events WHERE site_id = ? AND ts >= ? AND country IS NOT NULL
         GROUP BY country ORDER BY count DESC LIMIT 15",
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
        top_countries,
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

fn authorized(headers: &HeaderMap, state: &AppState) -> bool {
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
    user == state.config.dashboard_user && pass == state.config.dashboard_password
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
