DROP INDEX IF EXISTS idx_trend_results_window;

ALTER TABLE trend_results DROP COLUMN IF EXISTS window_signal;
ALTER TABLE trend_results DROP COLUMN IF EXISTS window_score;
ALTER TABLE trend_results DROP COLUMN IF EXISTS liquidity_tier;
ALTER TABLE trend_results DROP COLUMN IF EXISTS relative_liquidity;
ALTER TABLE trend_results DROP COLUMN IF EXISTS base_velocity;
ALTER TABLE trend_results DROP COLUMN IF EXISTS base_listings;
