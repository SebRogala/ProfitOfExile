DO $$ BEGIN RAISE EXCEPTION '20260723160019_league_scope_fragment_snapshots is forward-only; restore from a pre-migration backup'; END $$;
