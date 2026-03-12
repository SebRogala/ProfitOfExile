DROP TRIGGER IF EXISTS trg_strategies_updated_at ON strategies;
DROP TABLE IF EXISTS strategies;
-- NOTE: update_updated_at_column() is intentionally NOT dropped here.
-- It is shared infrastructure that may be used by other tables' triggers.
