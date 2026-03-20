-- Add tier-based mode columns and remove hardcoded threshold from font_snapshots.
ALTER TABLE font_snapshots
    DROP COLUMN IF EXISTS threshold,
    ADD COLUMN IF NOT EXISTS mode           TEXT         NOT NULL DEFAULT 'safe',
    ADD COLUMN IF NOT EXISTS thin_pool_gems INTEGER      NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS liquidity_risk TEXT         NOT NULL DEFAULT 'LOW';

-- Drop and recreate PK to include mode (time, color, variant, mode).
ALTER TABLE font_snapshots DROP CONSTRAINT IF EXISTS font_snapshots_pkey;
ALTER TABLE font_snapshots ADD PRIMARY KEY (time, color, variant, mode);
