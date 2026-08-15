use std::env;

#[derive(Clone)]
pub struct Config {
    pub database_path: String,
    pub default_site: String,
    pub allowed_origins: Vec<String>,
    pub dashboard_user: String,
    pub dashboard_password: String,
    pub bind: String,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            database_path: var("DATABASE_PATH", "checkpulse.db"),
            default_site: var("SITE_ID", "belderbos.dev"),
            allowed_origins: parse_origins(&var("ALLOWED_ORIGIN", "https://belderbos.dev")),
            dashboard_user: var("DASHBOARD_USER", "admin"),
            dashboard_password: required_var("DASHBOARD_PASSWORD"),
            bind: var("BIND", "0.0.0.0"),
            port: var("PORT", "8080").parse().unwrap_or(8080),
        }
    }
}

// ALLOWED_ORIGIN is comma-separated so one instance can serve several sites;
// each entry is normalized to match an incoming Origin header.
fn parse_origins(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches('/').to_lowercase())
        .collect()
}

fn var(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn required_var(key: &str) -> String {
    match env::var(key) {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("{key} must be set");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_origins_splits_trims_and_normalizes() {
        assert_eq!(
            parse_origins("https://belderbos.dev"),
            vec!["https://belderbos.dev"]
        );
        assert_eq!(
            parse_origins(" https://belderbos.dev , https://scriptertorust.com/ "),
            vec!["https://belderbos.dev", "https://scriptertorust.com"]
        );
        assert_eq!(
            parse_origins("HTTPS://Belderbos.Dev"),
            vec!["https://belderbos.dev"]
        );
        assert!(parse_origins("").is_empty());
        assert!(parse_origins(" , ").is_empty());
    }
}
