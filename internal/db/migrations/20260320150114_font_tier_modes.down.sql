-- Revert tier-based mode columns, restore threshold.
ALTER TABLE font_snapshots DROP CONSTRAINT IF EXISTS font_snapshots_pkey;

ALTER TABLE font_snapshots
    DROP COLUMN IF EXISTS mode,
    DROP COLUMN IF EXISTS thin_pool_gems,
    DROP COLUMN IF EXISTS liquidity_risk,
    ADD COLUMN IF NOT EXISTS threshold NUMERIC(10,2) NOT NULL DEFAULT 0;

ALTER TABLE font_snapshots ADD PRIMARY KEY (time, color, variant);
