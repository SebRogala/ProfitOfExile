DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM font_sessions WHERE device_id IS NULL OR pair_code IS NULL) THEN
        RAISE EXCEPTION 'font_sessions holds anonymised rows (NULL device_id or pair_code); cannot restore NOT NULL';
    END IF;
    IF EXISTS (SELECT 1 FROM lab_runs WHERE device_id IS NULL) THEN
        RAISE EXCEPTION 'lab_runs holds anonymised rows (NULL device_id); cannot restore NOT NULL';
    END IF;
    IF EXISTS (SELECT 1 FROM boss_loot_runs WHERE device_id IS NULL) THEN
        RAISE EXCEPTION 'boss_loot_runs holds anonymised rows (NULL device_id); cannot restore NOT NULL';
    END IF;
    IF EXISTS (SELECT 1 FROM merc_icon_templates WHERE device_id IS NULL) THEN
        RAISE EXCEPTION 'merc_icon_templates holds anonymised rows (NULL device_id); cannot restore NOT NULL';
    END IF;
END $$;

ALTER TABLE merc_icon_templates ALTER COLUMN device_id SET NOT NULL;

ALTER TABLE boss_loot_runs DROP COLUMN created_at;
ALTER TABLE boss_loot_runs DROP CONSTRAINT boss_loot_runs_device_id_fkey;
ALTER TABLE boss_loot_runs ADD CONSTRAINT boss_loot_runs_device_id_fkey
    FOREIGN KEY (device_id) REFERENCES devices(fingerprint);
ALTER TABLE boss_loot_runs ALTER COLUMN device_id SET NOT NULL;

ALTER TABLE lab_runs DROP COLUMN created_at;
ALTER TABLE lab_runs ALTER COLUMN device_id SET NOT NULL;

ALTER TABLE font_sessions ALTER COLUMN pair_code SET NOT NULL;
ALTER TABLE font_sessions ALTER COLUMN device_id SET NOT NULL;
