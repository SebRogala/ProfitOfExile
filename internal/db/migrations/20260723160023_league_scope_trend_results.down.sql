DO $$ BEGIN RAISE EXCEPTION '20260723160023_league_scope_trend_results is forward-only; restore from a pre-migration backup'; END $$;
