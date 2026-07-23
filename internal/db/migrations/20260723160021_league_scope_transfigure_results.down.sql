DO $$ BEGIN RAISE EXCEPTION '20260723160021_league_scope_transfigure_results is forward-only; restore from a pre-migration backup'; END $$;
