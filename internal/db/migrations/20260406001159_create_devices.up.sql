CREATE TABLE IF NOT EXISTS devices (
    fingerprint TEXT PRIMARY KEY,
    alias       TEXT,
    role        TEXT NOT NULL DEFAULT 'user',
    banned      BOOLEAN NOT NULL DEFAULT false,
    app_version TEXT,
    first_seen  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
