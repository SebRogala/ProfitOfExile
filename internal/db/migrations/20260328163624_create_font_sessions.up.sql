-- Font session tracking — crowd-sourced data from desktop app OCR.
-- One session per font encounter, multiple rounds per session.

CREATE TABLE font_sessions (
    id BIGSERIAL PRIMARY KEY,
    time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    lab_type TEXT NOT NULL,
    total_crafts INT NOT NULL,
    variant TEXT NOT NULL DEFAULT '20/20',
    device_id TEXT NOT NULL DEFAULT 'unknown',
    pair_code TEXT NOT NULL DEFAULT ''
);

CREATE INDEX idx_font_sessions_time ON font_sessions (time DESC);
CREATE INDEX idx_font_sessions_lab_type ON font_sessions (lab_type);

CREATE TABLE font_rounds (
    id BIGSERIAL PRIMARY KEY,
    session_id BIGINT NOT NULL REFERENCES font_sessions(id) ON DELETE CASCADE,
    round_number INT NOT NULL,
    craft_options JSONB NOT NULL DEFAULT '[]',
    option_chosen TEXT,
    gems_offered TEXT[],
    gem_picked TEXT,
    crafts_remaining INT
);

CREATE INDEX idx_font_rounds_session ON font_rounds (session_id);
