# Operations

Running a deployed checkpulse instance. Examples use the app name `checkpulse` and the volume `checkpulse_data` — substitute your own.

## Status and logs

```bash
fly status
fly logs
fly deploy          # ship code changes (new snippet, config, etc.)
```

## Pause and resume

`fly.toml` already suspends the machine when idle, so there's usually nothing to do. To stop serving entirely while keeping the app and its data:

```bash
fly apps suspend checkpulse
fly apps resume checkpulse
fly apps restart checkpulse       # or: fly machine restart <id>
```

## Rotate the dashboard password

Secrets can't be read back once set, only replaced.

```bash
fly secrets set DASHBOARD_PASSWORD='<a long random password>'
```

## Shell in and pull the database

```bash
fly ssh console
fly ssh sftp get /data/checkpulse.db ./backup.db
```

## Backups

Fly takes daily volume snapshots with 5-day retention by default.

```bash
fly volumes list
fly volumes snapshots list <volume-id>
```

For anything you want to keep longer than five days, pull the file down on a schedule — it's small.

## Querying custom events

The dashboard's **Top events** panel shows totals by name. Each row also stores the path the event fired from, so you can break any event down by article without tracking anything extra.

The runtime image has no `sqlite3`, so the workflow is: pull the DB locally, then query it with `just` (needs `sqlite3` on your machine).

```bash
just pull-db                       # fly ssh sftp get → ./checkpulse-prod.db (gitignored)
just events                        # all custom-event totals, last 30 days
just events cohort-python-agentic  # which articles drove that event, by path
just events cta-top 7              # top-CTA clicks in the last 7 days, by article
```

`just events NAME [DAYS] [DB]` — omit `NAME` for totals. Defaults are 30 days and `checkpulse-prod.db`; pass `checkpulse.db` to query local dev data.

`pull-db` copies the live file while the app may be writing to it. That's fine for aggregate counts; use a volume snapshot if you need a guaranteed-consistent dump.

Nothing stops you from querying the file directly — it's ordinary SQLite:

```bash
sqlite3 checkpulse-prod.db "SELECT path, COUNT(*) FROM events
  WHERE name IS NULL GROUP BY path ORDER BY 2 DESC LIMIT 20;"
```

## Wipe stats

Migrations recreate an empty database on restart.

```bash
fly ssh console -C 'rm -f /data/checkpulse.db /data/checkpulse.db-wal /data/checkpulse.db-shm'
fly apps restart checkpulse
```

## Tear down

Irreversible — destroys the app, machine, and volume.

```bash
fly apps destroy checkpulse
```
