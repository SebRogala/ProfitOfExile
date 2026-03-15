-- Add new analysis columns to font_snapshots.
-- Drop old columns that are no longer needed (min_val, max_val).
ALTER TABLE font_snapshots
    DROP COLUMN IF EXISTS min_val,
    DROP COLUMN IF EXISTS max_val,
    ADD COLUMN IF NOT EXISTS winners    INTEGER        NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS p_win      NUMERIC(10,6)  NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS avg_win    NUMERIC(10,2)  NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS input_cost NUMERIC(10,2)  NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS profit     NUMERIC(10,2)  NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS threshold  NUMERIC(10,2)  NOT NULL DEFAULT 0;
