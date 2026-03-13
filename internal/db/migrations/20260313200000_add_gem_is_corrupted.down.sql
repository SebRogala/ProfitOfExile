DELETE FROM gem_snapshots WHERE is_corrupted = true;

ALTER TABLE gem_snapshots DROP CONSTRAINT gem_snapshots_pkey;
ALTER TABLE gem_snapshots ADD PRIMARY KEY (time, name, variant);

DROP INDEX IF EXISTS idx_gem_snapshots_name_variant;
CREATE INDEX idx_gem_snapshots_name_variant ON gem_snapshots (name, variant, time DESC);

ALTER TABLE gem_snapshots DROP COLUMN is_corrupted;
