ALTER TABLE trend_results ADD COLUMN base_listings INTEGER NOT NULL DEFAULT 0;
ALTER TABLE trend_results ADD COLUMN base_velocity NUMERIC(10,4) NOT NULL DEFAULT 0;
ALTER TABLE trend_results ADD COLUMN relative_liquidity NUMERIC(6,4) NOT NULL DEFAULT 0;
ALTER TABLE trend_results ADD COLUMN liquidity_tier TEXT NOT NULL DEFAULT 'MED';
ALTER TABLE trend_results ADD COLUMN window_score NUMERIC(5,2) NOT NULL DEFAULT 0;
ALTER TABLE trend_results ADD COLUMN window_signal TEXT NOT NULL DEFAULT 'CLOSED';

CREATE INDEX idx_trend_results_window ON trend_results (time DESC, window_signal);
