-- PricedGems: how many of a colour pool carry a usable price at that variant.
-- Persisted so the DB-read path reports the same number the analyzer computed —
-- without it, any response served before the first font run after a restart
-- reports 0 priced for a fully priced pool.
-- NULL means "written before this column existed"; readers coalesce to pool.
ALTER TABLE font_snapshots ADD COLUMN IF NOT EXISTS priced_gems INTEGER;
