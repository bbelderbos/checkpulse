# checkpulse — Privacy-First Web Analytics

Lightweight, privacy-first web analytics with a Rust ingestion backend not storing any user identifiers. A single binary serves three things: the tracking snippet (`/script.js`), the ingestion API (`/api/event`), and a basic-auth dashboard (`/`). Data lives in one SQLite file.

## Privacy model

No cookies, no localStorage, no stored IPs. The visitor IP is used only to (a) derive a country and (b) feed a daily-salted SHA-256 hash for approximate unique counting, then discarded. The salt rotates every 24h so visitors cannot be correlated across days. `DNT: 1` requests are dropped. Each event also stores a coarse browser family (Chrome/Safari/Firefox/Edge/Other) and device type (desktop/mobile) parsed from the User-Agent — too coarse to fingerprint. Optional [custom events](#custom-events) are counted by name only, with no payload. This is similar to the Plausible model: privacy-respecting *aggregate* analytics.

## Security

Properties verified by review:

- **No SQL injection** — every query uses bound parameters (sqlx prepared statements); no request data is ever concatenated into SQL.
- **No stored XSS** — dashboard output is auto-escaped (Askama); the only unescaped values are server-generated chart numbers and dates.
- **No PII at rest** — IPs are never stored. The IP feeds a daily-salted SHA-256 hash (salt is random, in-memory, rotates every 24h and on restart) and is then discarded, so visitors can't be correlated across days or recovered later.
- **Authenticated dashboard** — basic auth over forced HTTPS; the app refuses to start without `DASHBOARD_PASSWORD`. Use a strong password (the dashboard itself is not rate-limited).
- **Abuse limits** — per-IP rate limiting (120 req/min) on `/api/event`, `Origin`/`Referer` allow-listing, and a 2 MB request-body cap.
- **Hardened runtime** — runs as a non-root user in the container; secrets come from env / Fly secrets and are gitignored locally.

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
| `ALLOWED_ORIGIN` | `https://belderbos.dev` | Enforced on `/api/event`: rejects events whose `Origin`/`Referer` doesn't match (empty = disabled) |
| `DASHBOARD_USER` | `admin` | Dashboard basic auth username |
| `DASHBOARD_PASSWORD` | _(required)_ | Dashboard basic auth password; the app refuses to start if unset |
| `GEOLITE_DB_PATH` | _(unset)_ | Path to `GeoLite2-Country.mmdb`; country disabled if absent |
| `BIND` / `PORT` | `0.0.0.0` / `8080` | Listen address |

## Add to a site (Zola)

Drop this into `templates/base.html` before `</head>`:

```html
<script src="https://checkpulse.fly.dev/script.js"></script>
```

The snippet derives its POST endpoint from its own host, so the same tag works in dev and prod.

## Custom events

The snippet exposes `window.checkpulse(name)` for tracking actions that aren't page loads (sign-ups, outbound clicks, etc.). Call it with a short event name:

```html
<button onclick="checkpulse('newsletter-signup')">Subscribe</button>
<a href="https://github.com/bbelderbos" onclick="checkpulse('outbound-github')">GitHub</a>
```

Each call records the event name and the current path — no other payload. Names are capped at 64 characters. Events show up in the dashboard's **Top events** panel, counted separately from page views. No call, no event; page views keep working on their own. Keep names free of personal data (they're stored verbatim).

### Querying events

The dashboard's **Top events** panel shows totals by name. Each event row also stores the page path it fired from, so you can break any event down by article without tracking anything extra — it's already in the `path` column.

The runtime image has no `sqlite3`, so the workflow is: pull the (small) DB locally, then query it with `just` (needs `sqlite3` on your machine):

```bash
just pull-db                       # fly ssh sftp get → ./checkpulse-prod.db (gitignored)
just events                        # all custom-event totals, last 30 days
just events cohort-python-agentic  # which articles drove that event, by path
just events cta-top 7              # top-CTA clicks in the last 7 days, by article
```

`just events NAME [DAYS] [DB]` — omit `NAME` for totals; defaults are 30 days and `checkpulse-prod.db` (pass a local `checkpulse.db` for dev data). `pull-db` copies the live file while the app may be writing; fine for aggregate counts, but use a volume snapshot if you need a guaranteed-consistent dump.

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

## Operations (Fly.io)

Day-to-day management of the deployed app (`checkpulse`):

```bash
# Status & logs
fly status
fly logs

# Ship code changes (new snippet, Origin check, etc.)
fly deploy

# Pause / resume billing-relevant compute
fly apps suspend checkpulse     # stop serving, keep app + data; resume later
fly apps resume checkpulse
fly machine restart <id>        # or: fly apps restart checkpulse

# Rotate the dashboard password (can't be read back once set)
fly secrets set DASHBOARD_PASSWORD=...

# Shell into the running machine
fly ssh console
fly ssh sftp get /data/checkpulse.db ./backup.db   # pull a DB copy (see issue #2)

# Wipe stats for a clean slate (migrations recreate an empty DB on restart)
fly ssh console -C 'rm -f /data/checkpulse.db /data/checkpulse.db-wal /data/checkpulse.db-shm'
fly apps restart checkpulse

# Volume snapshots (daily, 5-day retention by default)
fly volumes list
fly volumes snapshots list <volume-id>

# Tear everything down (app, machine, volume — irreversible)
fly apps destroy checkpulse
```

## Develop

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```
