DO $$ BEGIN RAISE EXCEPTION '20260723160018_league_scope_currency_snapshots is forward-only; restore from a pre-migration backup'; END $$;
