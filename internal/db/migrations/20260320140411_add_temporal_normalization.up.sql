ALTER TABLE market_context ADD COLUMN temporal_coefficient DOUBLE PRECISION NOT NULL DEFAULT 1.0;
ALTER TABLE market_context ADD COLUMN temporal_mode TEXT NOT NULL DEFAULT 'none';
ALTER TABLE market_context ADD COLUMN temporal_buckets JSONB NOT NULL DEFAULT '{}';
