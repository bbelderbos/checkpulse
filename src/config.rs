use std::env;

#[derive(Clone)]
pub struct Config {
    pub database_path: String,
    pub site_id: String,
    pub allowed_origin: String,
    pub dashboard_user: String,
    pub dashboard_password: String,
    pub geolite_db_path: Option<String>,
    pub bind: String,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            database_path: var("DATABASE_PATH", "checkpulse.db"),
            site_id: var("SITE_ID", "belderbos.dev"),
            allowed_origin: var("ALLOWED_ORIGIN", "https://belderbos.dev"),
            dashboard_user: var("DASHBOARD_USER", "admin"),
            dashboard_password: var("DASHBOARD_PASSWORD", "changeme"),
            geolite_db_path: env::var("GEOLITE_DB_PATH").ok().filter(|s| !s.is_empty()),
            bind: var("BIND", "0.0.0.0"),
            port: var("PORT", "8080").parse().unwrap_or(8080),
        }
    }
}

fn var(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}
