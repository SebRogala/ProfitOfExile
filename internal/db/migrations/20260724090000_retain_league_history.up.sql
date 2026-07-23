-- Retention policies drop chunks by time alone. With league-scoped data they
-- delete an archived league's earliest observations, including league start,
-- which is the window POE-117 exists to preserve. Compression policies stay.
SELECT remove_retention_policy('currency_snapshots', if_exists => TRUE);
SELECT remove_retention_policy('dedication_snapshots', if_exists => TRUE);
SELECT remove_retention_policy('font_snapshots', if_exists => TRUE);
SELECT remove_retention_policy('fragment_snapshots', if_exists => TRUE);
SELECT remove_retention_policy('gem_features', if_exists => TRUE);
SELECT remove_retention_policy('gem_signals', if_exists => TRUE);
SELECT remove_retention_policy('gem_snapshots', if_exists => TRUE);
SELECT remove_retention_policy('market_context', if_exists => TRUE);
SELECT remove_retention_policy('quality_results', if_exists => TRUE);
SELECT remove_retention_policy('transfigure_results', if_exists => TRUE);
SELECT remove_retention_policy('trend_results', if_exists => TRUE);
