BEGIN;

CREATE TABLE leagues (
    id               TEXT PRIMARY KEY,
    display_name     TEXT NOT NULL UNIQUE,
    starts_at        TIMESTAMPTZ,
    ends_at          TIMESTAMPTZ,
    collection_state TEXT NOT NULL CHECK (collection_state IN ('prepared', 'collecting', 'archived')),
    prepared_at      TIMESTAMPTZ,
    activated_at     TIMESTAMPTZ,
    archived_at      TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE runtime_config (
    singleton     BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    active_league TEXT NOT NULL REFERENCES leagues(id),
    revision      BIGINT NOT NULL DEFAULT 1,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO leagues (id, display_name, collection_state, prepared_at, activated_at)
VALUES ('Mirage', 'Mirage', 'collecting', NOW(), NOW());

INSERT INTO runtime_config (active_league, revision)
VALUES ('Mirage', 1);

COMMIT;
