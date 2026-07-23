DO $$ BEGIN RAISE EXCEPTION '20260723160026_league_scope_dedication_snapshots is forward-only; restore from a pre-migration backup'; END $$;
