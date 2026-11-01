-- POE-317 final Operator ruling: retain versioned empirical boss observations
-- across rollover. League identity remains explicit; profile versions separate
-- changed loot pools. This is not a claim of permanent league invariance.
-- Deliberately excluded from league.ScopedTables and the ADR-011 wipe set.
CREATE TABLE boss_loot_runs (
    id BIGSERIAL NOT NULL,
    league TEXT NOT NULL REFERENCES leagues(id),
    device_id TEXT NOT NULL REFERENCES devices(fingerprint),
    boss TEXT NOT NULL,
    profile_key TEXT NOT NULL,
    profile_version INTEGER NOT NULL CHECK (profile_version > 0),
    run_number BIGINT NOT NULL CHECK (run_number > 0),
    seed TEXT,
    date TIMESTAMPTZ,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    items JSONB NOT NULL CHECK (
        jsonb_typeof(items) = 'object'
        AND items ? 'quantities' AND jsonb_typeof(items->'quantities') = 'object'
        AND items ? 'counted' AND jsonb_typeof(items->'counted') = 'array'
    ),
    PRIMARY KEY (league, id),
    UNIQUE (device_id, boss, profile_key, profile_version, run_number)
);
CREATE INDEX boss_loot_runs_profile ON boss_loot_runs (boss, profile_key, profile_version);
