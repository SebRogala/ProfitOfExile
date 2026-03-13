-- Drop continuous aggregates first (they depend on base hypertables)
DROP MATERIALIZED VIEW IF EXISTS gem_snapshots_daily CASCADE;
DROP MATERIALIZED VIEW IF EXISTS gem_snapshots_hourly CASCADE;

-- Remove retention policies
SELECT remove_retention_policy('gem_snapshots', if_exists => true);
SELECT remove_retention_policy('font_snapshots', if_exists => true);
SELECT remove_retention_policy('exchange_snapshots', if_exists => true);
SELECT remove_retention_policy('gcp_snapshots', if_exists => true);

-- Remove compression policies
SELECT remove_compression_policy('gem_snapshots', if_exists => true);
SELECT remove_compression_policy('font_snapshots', if_exists => true);
SELECT remove_compression_policy('exchange_snapshots', if_exists => true);
SELECT remove_compression_policy('gcp_snapshots', if_exists => true);

-- Disable compression on hypertables
ALTER TABLE gem_snapshots SET (timescaledb.compress = false);
ALTER TABLE font_snapshots SET (timescaledb.compress = false);
ALTER TABLE exchange_snapshots SET (timescaledb.compress = false);
ALTER TABLE gcp_snapshots SET (timescaledb.compress = false);
