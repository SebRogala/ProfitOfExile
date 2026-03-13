-- Remove policies before dropping tables
SELECT remove_retention_policy('exchange_snapshots', if_exists => true);
SELECT remove_retention_policy('gcp_snapshots', if_exists => true);
SELECT remove_compression_policy('exchange_snapshots', if_exists => true);
SELECT remove_compression_policy('gcp_snapshots', if_exists => true);

DROP TABLE IF EXISTS exchange_snapshots CASCADE;
DROP TABLE IF EXISTS gcp_snapshots CASCADE;
