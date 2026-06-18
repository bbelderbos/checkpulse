CREATE TABLE events (
    id           INTEGER PRIMARY KEY,
    site_id      TEXT    NOT NULL,
    ts           INTEGER NOT NULL,
    path         TEXT    NOT NULL,
    referrer     TEXT,
    country      TEXT,
    visitor_hash TEXT    NOT NULL
);

CREATE INDEX idx_events_site_ts ON events (site_id, ts);
