ALTER TABLE gem_snapshots ADD COLUMN is_corrupted BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE gem_snapshots DROP CONSTRAINT gem_snapshots_pkey;
ALTER TABLE gem_snapshots ADD PRIMARY KEY (time, name, variant, is_corrupted);

DROP INDEX IF EXISTS idx_gem_snapshots_name_variant;
CREATE INDEX idx_gem_snapshots_name_variant ON gem_snapshots (name, variant, is_corrupted, time DESC);
