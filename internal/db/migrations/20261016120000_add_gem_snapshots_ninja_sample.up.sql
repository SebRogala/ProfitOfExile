-- POE-296 D5: nullable sample fields preserve unknown values for historical rows.
ALTER TABLE gem_snapshots ADD COLUMN sample_count INTEGER;
ALTER TABLE gem_snapshots ADD COLUMN sparkline_empty BOOLEAN;
