DO $$ BEGIN RAISE EXCEPTION '20260723160028_league_scope_market_context is forward-only; restore from a pre-migration backup'; END $$;
