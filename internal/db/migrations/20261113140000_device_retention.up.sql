-- POE-396: rows outlive their device link. A retention job (WI-2) nulls
-- device_id/pair_code after 2 years, and deleting an inactive device must not be
-- blocked by its boss-loot rows. lab_runs.started_at and boss_loot_runs.date are
-- client-sent, so retention age is measured from a server-set created_at.
ALTER TABLE font_sessions ALTER COLUMN device_id DROP NOT NULL;
ALTER TABLE font_sessions ALTER COLUMN pair_code DROP NOT NULL;

ALTER TABLE lab_runs ALTER COLUMN device_id DROP NOT NULL;
ALTER TABLE lab_runs ADD COLUMN created_at TIMESTAMPTZ;
UPDATE lab_runs SET created_at = LEAST(started_at, NOW());
ALTER TABLE lab_runs ALTER COLUMN created_at SET DEFAULT NOW();
ALTER TABLE lab_runs ALTER COLUMN created_at SET NOT NULL;

ALTER TABLE boss_loot_runs ALTER COLUMN device_id DROP NOT NULL;
DO $$
DECLARE
    fk_name TEXT;
BEGIN
    SELECT c.conname INTO fk_name
    FROM pg_constraint c
    JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = ANY (c.conkey)
    WHERE c.conrelid = 'boss_loot_runs'::regclass
      AND c.contype = 'f'
      AND c.confrelid = 'devices'::regclass
      AND a.attname = 'device_id';
    IF fk_name IS NULL THEN
        RAISE EXCEPTION 'boss_loot_runs: no foreign key from device_id to devices';
    END IF;
    EXECUTE format('ALTER TABLE boss_loot_runs DROP CONSTRAINT %I', fk_name);
END $$;
ALTER TABLE boss_loot_runs ADD CONSTRAINT boss_loot_runs_device_id_fkey
    FOREIGN KEY (device_id) REFERENCES devices(fingerprint) ON DELETE SET NULL;
ALTER TABLE boss_loot_runs ADD COLUMN created_at TIMESTAMPTZ;
UPDATE boss_loot_runs SET created_at = NOW();
ALTER TABLE boss_loot_runs ALTER COLUMN created_at SET DEFAULT NOW();
ALTER TABLE boss_loot_runs ALTER COLUMN created_at SET NOT NULL;

ALTER TABLE merc_icon_templates ALTER COLUMN device_id DROP NOT NULL;
