CREATE TABLE IF NOT EXISTS strategies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    edit_token TEXT NOT NULL UNIQUE CHECK (length(edit_token) >= 16),
    name TEXT NOT NULL,
    league TEXT NOT NULL,
    tree JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_strategies_league ON strategies (league);

CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_strategies_updated_at
    BEFORE UPDATE ON strategies
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
