CREATE INDEX idx_gem_snapshots_league_transfigured_name
    ON gem_snapshots (league, is_transfigured, name)
    WITH (timescaledb.transaction_per_chunk);
