DO $$ BEGIN RAISE EXCEPTION '20260723160022_league_scope_quality_results is forward-only; restore from a pre-migration backup'; END $$;
