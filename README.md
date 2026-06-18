# checkpulse — Privacy-First Web Analytics

Lightweight, GDPR-compliant web analytics with a Rust ingestion backend. A single binary
serves three things: the tracking snippet (`/script.js`), the ingestion API (`/api/event`),
and a basic-auth dashboard (`/`). Data lives in one SQLite file.

## Privacy model

No cookies, no localStorage, no stored IPs. The visitor IP is used only to (a) derive a
country and (b) feed a daily-salted SHA-256 hash for approximate unique counting, then
discarded. The salt rotates every 24h so visitors cannot be correlated across days. `DNT: 1`
requests are dropped. This is the Plausible model: privacy-respecting *aggregate* analytics.

## Run locally

```bash
cp .env.example .env   # adjust DASHBOARD_PASSWORD etc.
DASHBOARD_USER=admin DASHBOARD_PASSWORD=secret PORT=8099 cargo run
```

- Dashboard: http://localhost:8099/ (basic auth)
- Send a test event:
  ```bash
  curl -X POST localhost:8099/api/event -H 'User-Agent: test' \
    -d '{"path":"/hello","referrer":"https://news.ycombinator.com/"}'
  ```

## Config (env vars)

| Var | Default | Notes |
|-----|---------|-------|
| `DATABASE_PATH` | `checkpulse.db` | SQLite file path |
| `SITE_ID` | `belderbos.dev` | Tag stored on every event |
| `ALLOWED_ORIGIN` | `https://belderbos.dev` | Reserved for CORS tightening |
| `DASHBOARD_USER` / `DASHBOARD_PASSWORD` | `admin` / `changeme` | Dashboard basic auth |
| `GEOLITE_DB_PATH` | _(unset)_ | Path to `GeoLite2-Country.mmdb`; country disabled if absent |
| `BIND` / `PORT` | `0.0.0.0` / `8080` | Listen address |

## Add to a site (Zola)

Drop this into `templates/base.html` before `</head>`:

```html
<script src="https://checkpulse.fly.dev/script.js"></script>
```

The snippet derives its POST endpoint from its own host, so the same tag works in dev and prod.

## Country breakdown (optional)

1. Create a free MaxMind account, download `GeoLite2-Country.mmdb`.
2. Place it where `GEOLITE_DB_PATH` points (on Fly: upload to the `/data` volume).
   Country fills in automatically once the file is present; no code change needed.

## Deploy (Fly.io)

```bash
fly launch --no-deploy        # or `fly apps create checkpulse` if fly.toml already set
fly volumes create checkpulse_data --region ams --size 1
fly secrets set DASHBOARD_USER=... DASHBOARD_PASSWORD=...
fly deploy
```

## Develop

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```
